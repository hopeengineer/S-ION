FROM node:20-slim

WORKDIR /app

# Install build deps for better-sqlite3
RUN apt-get update && apt-get install -y python3 make g++ && rm -rf /var/lib/apt/lists/*

COPY package*.json ./
RUN npm install

COPY tsconfig.json ./
COPY src/ ./src/
RUN npm run build

# Create data directory for Railway Volume
RUN mkdir -p /data

ENV DB_PATH=/data/sion.db
ENV PORT=3500

EXPOSE 3500

CMD ["node", "dist/index.js"]
