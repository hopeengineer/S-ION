use std::path::{Path, PathBuf};
use tokio::sync::watch;

// ──────────────────────────────────────────────────
// Model Provisioner
// ──────────────────────────────────────────────────

/// Status of the model provisioning process.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ProvisionStatus {
    pub ready: bool,
    pub downloading: bool,
    pub progress_bytes: u64,
    pub total_bytes: u64,
    pub speed_bps: u64,
    pub model_name: String,
    pub error: Option<String>,
}

impl Default for ProvisionStatus {
    fn default() -> Self {
        Self {
            ready: false,
            downloading: false,
            progress_bytes: 0,
            total_bytes: 0,
            speed_bps: 0,
            model_name: "BGE-M3-INT8".into(),
            error: None,
        }
    }
}

/// Required files for the sovereign embedding model.
const MODEL_FILES: &[&str] = &[
    "model_quantized.onnx",
    "tokenizer.json",
    "config.json",
];

/// Remote URLs for each model file (HuggingFace Hub — INT8 quantized community build).
const MODEL_BASE_URL: &str = "https://huggingface.co/gpahal/bge-m3-onnx-int8/resolve/main";

pub struct ModelProvisioner {
    models_dir: PathBuf,
    status_tx: watch::Sender<ProvisionStatus>,
    status_rx: watch::Receiver<ProvisionStatus>,
}

impl ModelProvisioner {
    /// Initialize the provisioner. Checks the OS-native models directory.
    pub fn new() -> Result<Self, String> {
        let data_dir = dirs::data_local_dir()
            .ok_or("Cannot determine OS data directory")?;
        let models_dir = data_dir.join("com.s-ion.dev").join("models");
        std::fs::create_dir_all(&models_dir)
            .map_err(|e| format!("Failed to create models dir: {}", e))?;

        let (status_tx, status_rx) = watch::channel(ProvisionStatus::default());

        let prov = Self { models_dir, status_tx, status_rx };

        // Check if already provisioned
        if prov.check_files() {
            prov.status_tx.send_modify(|s| {
                s.ready = true;
                s.downloading = false;
            });
            println!("🧠 Model Provisioner: BGE-M3 INT8 ready at {}", prov.models_dir.display());
        } else {
            println!("🧠 Model Provisioner: BGE-M3 INT8 not found. Will download on first use.");
        }

        Ok(prov)
    }

    /// Check if all required model files exist.
    pub fn check_files(&self) -> bool {
        MODEL_FILES.iter().all(|f| self.models_dir.join(f).exists())
    }

    /// Whether the model is ready for inference.
    pub fn is_ready(&self) -> bool {
        self.status_rx.borrow().ready
    }

    /// Get a clone of the status receiver for frontend updates.
    pub fn status_receiver(&self) -> watch::Receiver<ProvisionStatus> {
        self.status_rx.clone()
    }

    /// Get model file path.
    pub fn model_path(&self) -> PathBuf {
        self.models_dir.join("model_quantized.onnx")
    }

    /// Get tokenizer file path.
    pub fn tokenizer_path(&self) -> PathBuf {
        self.models_dir.join("tokenizer.json")
    }

    /// Trigger the model download. Emits progress via the watch channel.
    pub async fn provision(&self) -> Result<(), String> {
        if self.is_ready() {
            return Ok(());
        }

        self.status_tx.send_modify(|s| {
            s.downloading = true;
            s.error = None;
        });

        let client = reqwest::Client::new();

        for file_name in MODEL_FILES {
            let file_path = self.models_dir.join(file_name);
            if file_path.exists() {
                continue;
            }

            let url = format!("{}/{}", MODEL_BASE_URL, file_name);
            println!("🧠 Downloading: {} → {}", url, file_path.display());

            match self.download_file(&client, &url, &file_path).await {
                Ok(_) => {
                    println!("✅ Downloaded: {}", file_name);
                }
                Err(e) => {
                    self.status_tx.send_modify(|s| {
                        s.downloading = false;
                        s.error = Some(format!("Failed to download {}: {}", file_name, e));
                    });
                    return Err(format!("Download failed: {}", e));
                }
            }
        }

        // Validate all files present
        if self.check_files() {
            self.status_tx.send_modify(|s| {
                s.ready = true;
                s.downloading = false;
            });
            println!("🧠 Provisioning complete. BGE-M3 INT8 ready.");
            Ok(())
        } else {
            Err("Provisioning validation failed: files missing after download".into())
        }
    }

    async fn download_file(
        &self, client: &reqwest::Client, url: &str, dest: &Path,
    ) -> Result<(), String> {
        use futures_util::StreamExt;

        let resp = client.get(url)
            .send().await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("HTTP {}: {}", resp.status(), url));
        }

        let total = resp.content_length().unwrap_or(0);
        self.status_tx.send_modify(|s| {
            s.total_bytes = total;
            s.progress_bytes = 0;
        });

        let mut stream = resp.bytes_stream();
        let mut file = tokio::fs::File::create(dest).await
            .map_err(|e| format!("File create failed: {}", e))?;

        let mut downloaded = 0u64;
        let start = std::time::Instant::now();

        use tokio::io::AsyncWriteExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("Stream error: {}", e))?;
            file.write_all(&chunk).await
                .map_err(|e| format!("Write error: {}", e))?;

            downloaded += chunk.len() as u64;
            let elapsed = start.elapsed().as_secs_f64().max(0.001);
            let speed = (downloaded as f64 / elapsed) as u64;

            self.status_tx.send_modify(|s| {
                s.progress_bytes = downloaded;
                s.speed_bps = speed;
            });
        }

        file.flush().await.map_err(|e| format!("Flush error: {}", e))?;
        Ok(())
    }
}
