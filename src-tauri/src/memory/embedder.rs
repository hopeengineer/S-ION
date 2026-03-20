use std::path::Path;
use std::sync::Mutex;

// ──────────────────────────────────────────────────
// Unified Embedder (Sovereign → Cloud Fallback)
// ──────────────────────────────────────────────────

/// The selected hardware execution provider.
#[derive(Debug, Clone)]
pub enum ExecutionProvider {
    CoreML,                       // macOS Apple Neural Engine
    #[cfg(target_os = "windows")]
    DirectML,                     // Windows GPU/NPU
    #[cfg(target_os = "linux")]
    CUDA,                         // Linux NVIDIA
    CPU,                          // Universal fallback
}

impl std::fmt::Display for ExecutionProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CoreML => write!(f, "Apple Neural Engine (CoreML)"),
            #[cfg(target_os = "windows")]
            Self::DirectML => write!(f, "DirectML (GPU/NPU)"),
            #[cfg(target_os = "linux")]
            Self::CUDA => write!(f, "CUDA (NVIDIA)"),
            Self::CPU => write!(f, "CPU"),
        }
    }
}

pub struct Embedder {
    session: Mutex<Option<ort::session::Session>>,
    tokenizer: Mutex<Option<tokenizers::Tokenizer>>,
    provider: ExecutionProvider,
    gemini_key: Option<String>,
}

impl Embedder {
    pub fn new(gemini_key: Option<String>) -> Self {
        let provider = Self::hardware_handshake();
        println!("⚡ Embedder: {} selected", provider);
        Self {
            session: Mutex::new(None),
            tokenizer: Mutex::new(None),
            provider,
            gemini_key,
        }
    }

    /// Probe for the fastest available ONNX Execution Provider.
    fn hardware_handshake() -> ExecutionProvider {
        #[cfg(target_os = "macos")]
        {
            println!("⚡ Hardware Handshake: CoreML available (macOS)");
            return ExecutionProvider::CoreML;
        }

        #[cfg(target_os = "windows")]
        {
            println!("⚡ Hardware Handshake: DirectML available (Windows)");
            return ExecutionProvider::DirectML;
        }

        #[cfg(target_os = "linux")]
        {
            if std::path::Path::new("/usr/lib/libcuda.so").exists()
                || std::env::var("CUDA_HOME").is_ok()
            {
                println!("⚡ Hardware Handshake: CUDA available (Linux)");
                return ExecutionProvider::CUDA;
            }
        }

        #[allow(unreachable_code)]
        {
            println!("⚡ Hardware Handshake: CPU fallback");
            ExecutionProvider::CPU
        }
    }

    /// Initialize the local ONNX session with the downloaded model.
    pub async fn init_local(&self, model_path: &Path, tokenizer_path: &Path) -> Result<(), String> {
        let tok = tokenizers::Tokenizer::from_file(tokenizer_path)
            .map_err(|e| format!("Tokenizer load failed: {}", e))?;
        *self
            .tokenizer
            .lock()
            .map_err(|e| format!("Lock error: {}", e))? = Some(tok);

        let session = self.build_session(model_path)?;
        *self
            .session
            .lock()
            .map_err(|e| format!("Lock error: {}", e))? = Some(session);

        println!("⚡ Local ONNX embedder initialized via {}", self.provider);
        Ok(())
    }

    fn build_session(&self, model_path: &Path) -> Result<ort::session::Session, String> {
        let builder = ort::session::Session::builder()
            .map_err(|e| format!("Session builder error: {}", e))?;

        // Register the platform-specific execution provider
        #[cfg(target_os = "macos")]
        let mut builder = if matches!(self.provider, ExecutionProvider::CoreML) {
            builder
                .with_execution_providers([
                    ort::execution_providers::CoreMLExecutionProvider::default().build(),
                ])
                .map_err(|e| format!("CoreML EP error: {}", e))?
        } else {
            builder
        };

        #[cfg(target_os = "windows")]
        let mut builder = if matches!(self.provider, ExecutionProvider::DirectML) {
            builder
                .with_execution_providers([
                    ort::execution_providers::DirectMLExecutionProvider::default().build(),
                ])
                .map_err(|e| format!("DirectML EP error: {}", e))?
        } else {
            builder
        };

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let mut builder = builder;

        // ort rc12: commit_from_file doesn't exist, use commit_from_memory
        let model_bytes =
            std::fs::read(model_path).map_err(|e| format!("Failed to read model file: {}", e))?;
        builder
            .commit_from_memory(&model_bytes)
            .map_err(|e| format!("Model load error: {}", e))
    }

    pub fn is_local_ready(&self) -> bool {
        let s = self.session.lock().map(|g| g.is_some()).unwrap_or(false);
        let t = self.tokenizer.lock().map(|g| g.is_some()).unwrap_or(false);
        s && t
    }

    /// Embed text. Routes to local ONNX if available, otherwise Gemini cloud.
    pub async fn embed_text(&self, text: &str) -> Result<Vec<f32>, String> {
        if self.is_local_ready() {
            self.embed_local(text)
        } else if self.gemini_key.is_some() {
            self.embed_gemini(text).await
        } else {
            Err("No embedder available: local model not ready and no Gemini API key".into())
        }
    }

    /// Local ONNX embedding via BGE-M3.
    fn embed_local(&self, text: &str) -> Result<Vec<f32>, String> {
        let tok_guard = self
            .tokenizer
            .lock()
            .map_err(|e| format!("Tok lock: {}", e))?;
        let tokenizer = tok_guard.as_ref().ok_or("Tokenizer not initialized")?;
        let mut sess_guard = self
            .session
            .lock()
            .map_err(|e| format!("Sess lock: {}", e))?;
        let session = sess_guard.as_mut().ok_or("ONNX session not initialized")?;

        // Tokenize
        let encoding = tokenizer
            .encode(text, true)
            .map_err(|e| format!("Tokenize error: {}", e))?;

        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
        let attention_mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&m| m as i64)
            .collect();
        let token_type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|&t| t as i64).collect();
        let seq_len = input_ids.len();

        // Build input tensors
        let ids_val = ort::value::Value::from_array(([1usize, seq_len], input_ids))
            .map_err(|e| format!("Input ids tensor error: {}", e))?;
        let mask_val = ort::value::Value::from_array(([1usize, seq_len], attention_mask))
            .map_err(|e| format!("Attention mask tensor error: {}", e))?;
        let types_val = ort::value::Value::from_array(([1usize, seq_len], token_type_ids))
            .map_err(|e| format!("Token types tensor error: {}", e))?;

        // ort::inputs! returns a Vec, not a Result
        let inputs = ort::inputs![
            "input_ids" => ids_val,
            "attention_mask" => mask_val,
            "token_type_ids" => types_val,
        ];

        let outputs = session
            .run(inputs)
            .map_err(|e| format!("Inference error: {}", e))?;

        // Extract the [CLS] token embedding
        let output = &outputs[0];
        // ort rc12: try_extract_tensor returns (&Shape, &[f32]) tuple
        let (shape, data) = output
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Tensor extract error: {}", e))?;

        // Shape is [1, seq_len, hidden_dim]. Get first hidden_dim values for [CLS]
        let dims: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
        let hidden_dim = if dims.len() == 3 {
            dims[2]
        } else {
            *dims.last().unwrap_or(&1024)
        };
        let embedding: Vec<f32> = data[..hidden_dim].to_vec();

        // L2 normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            Ok(embedding.iter().map(|x| x / norm).collect())
        } else {
            Ok(embedding)
        }
    }

    /// Gemini Embedding cloud fallback (1024-dim to match local space).
    async fn embed_gemini(&self, text: &str) -> Result<Vec<f32>, String> {
        let key = self.gemini_key.as_ref().ok_or("No Gemini API key")?;
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/text-embedding-004:embedContent?key={}",
            key
        );

        let body = serde_json::json!({
            "model": "models/text-embedding-004",
            "content": { "parts": [{ "text": text }] },
            "outputDimensionality": 1024
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Gemini embed request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Gemini embed error {}: {}", status, text));
        }

        let result: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        let values = result["embedding"]["values"]
            .as_array()
            .ok_or("No embedding values in Gemini response")?;

        let embedding: Vec<f32> = values
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        if embedding.is_empty() {
            return Err("Empty embedding from Gemini".into());
        }

        // L2 normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            Ok(embedding.iter().map(|x| x / norm).collect())
        } else {
            Ok(embedding)
        }
    }
}
