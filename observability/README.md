# Observabilidade opcional

O Compose principal continua **sem** Collector, Prometheus e Grafana. Esses serviços só sobem quando o perfil `observability` é ativado explicitamente.

## Como usar

```bash
common/dc.sh <produto> --observability up -d
common/dc.sh <produto> --observability ps
common/dc.sh <produto> --observability logs
common/dc.sh <produto> --observability down
```

Substitua `<produto>` pelo diretório do produto em `products/` (por exemplo `accounting`, `trivia` ou um novo produto).

## Portas e serviços (apenas no perfil)

- **OpenTelemetry Collector**: OTLP gRPC `4317`, OTLP HTTP `4318`, endpoint Prometheus interno/exportado `8889`
- **Prometheus**: `http://localhost:9090` (retenção básica: 7 dias ou 512MB)
- **Grafana**: `http://localhost:3000` (datasource Prometheus provisionado automaticamente)

## Variáveis e segurança

- `GRAFANA_ADMIN_USER` (padrão: `admin`)
- `GRAFANA_ADMIN_PASSWORD` (padrão: `change-me`) — **troque antes de usar em ambiente compartilhado/produção**.

Nenhuma credencial real deve ser versionada. Use o `.env` do produto para valores locais.

## Dados e limites

- Volumes nomeados por projeto Compose: `prometheus-data` e `grafana-data`.
- O Collector **não** é backend de armazenamento de longo prazo; ele recebe/encaminha telemetria conforme configuração.

## Estado atual das métricas

O perfil fornece infraestrutura para receber métricas OTLP e visualizá-las via Prometheus/Grafana. **A aplicação/TUI ainda não emite métricas de negócio via OpenTelemetry** por padrão; para ter métricas da aplicação é necessário instrumentá-la para enviar OTLP ao Collector (`4317/4318`).
