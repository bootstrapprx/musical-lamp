# Observabilidade opcional

O Compose principal continua sem portas de observabilidade e sem Grafana, Prometheus ou OpenTelemetry. Para habilitar o perfil para um produto, use `common/dc.sh <produto> --observability up -d`; para consultar, use `common/dc.sh <produto> --observability ps`; para desligar, use `common/dc.sh <produto> --observability down`.

O perfil inicia o OpenTelemetry Collector, que recebe OTLP em `4317` (gRPC) e `4318` (HTTP), expõe métricas Prometheus em `8889`, e encaminha essas métricas para o Prometheus. O Grafana fica disponível em `http://localhost:3000` e o Prometheus em `http://localhost:9090`. O dashboard e a fonte de dados do Grafana ainda são configurados pelo operador.

Antes de usar em ambiente compartilhado, defina `GRAFANA_ADMIN_PASSWORD` no `.env` do produto. O valor padrão `change-me` existe apenas para tornar o perfil fácil de experimentar localmente e não deve ser usado em produção. Os volumes `prometheus-data` e `grafana-data` são separados por projeto Compose.
