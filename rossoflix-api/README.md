# Servidor Rust de Alta Performance (Axum) — Listar filmes via IMDb/OMDb

> **Observação importante sobre a API do IMDb**: o IMDb não oferece uma API pública oficial gratuita. A alternativa mais comum é a **OMDb API** ([https://www.omdbapi.com](https://www.omdbapi.com)), que usa *IDs do IMDb* e retorna dados compatíveis para buscas e detalhes de filmes. Abaixo está um servidor Rust de alta performance usando **Axum** + **Tokio**, com **cache em memória (moka)**, **compressão**, **timeouts**, **pooling de conexões** e **tratamento de erros**. Basta informar a variável de ambiente `OMDB_API_KEY`.

---

## Estrutura

```
.
├── Cargo.toml
├── Dockerfile
├── .env
└── src/
    └── main.rs
```

---

## Como rodar

### 1) Localmente

```bash
cargo run --release
# Servirá em 0.0.0.0:8080 (ou PORT do .env)
```

### 2) Docker

```bash
docker build -t rossoflix-api .
docker run --rm -p 8080:8080 -e OMDB_API_KEY=SUACHAVE rossoflix-api
```

---

## Exemplos de uso (HTTP)

### Health

```bash
curl -s http://localhost:8080/health | jq
```

### Buscar filmes por nome (com paginação e tipo)

```bash
curl -s "http://localhost:8080/search?q=Matrix&page=1&type=movie" | jq
```

### Detalhes por IMDb ID

```bash
curl -s "http://localhost:8080/movie/tt0133093" | jq
```

---

## Notas de performance

* **Axum + Tokio**: alto throughput e baixa latência.
* **`reqwest` com pooling**: conexões HTTP reutilizadas e compressão (gzip/br) habilitada.
* **Cache `moka` (TTL 60s)**: reduz chamadas à API externa e melhora P99.
* **`tower-http`**: compressão de respostas e tracing estruturado.
* **Timeouts**: fim a fim (cliente e serviço) para evitar *queue buildup*.

> Para cargas muito altas, considere adicionar **rate limiting** (ex.: `tower-governor`), **observabilidade** (OpenTelemetry), **cache distribuído** (Redis) e **sharding** por chave de cache.
