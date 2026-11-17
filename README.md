# frappe-deployer

TUI em Rust (Ratatui/Crossterm + Tokio) para administrar produtos Docker Compose em `products/<produto>`, com foco em ciclo de vida de `.env`, pre-flight/Doctor, operações da stack e telemetria local do Docker.

## Requisitos

- Rust toolchain (edition 2021)
- Terminal interativo (TTY)
- Docker + Docker Compose plugin (opcional para operar stacks; a TUI inicia sem Docker, mas funcionalidades Docker ficam indisponíveis)

## Instalação e execução

Na raiz do repositório:

```bash
cargo build
cargo run
```

Também é possível informar a raiz do projeto:

```bash
cargo run -- /caminho/para/musical-lamp
```

Variável de ambiente suportada:

- `MUSICAL_LAMP_ROOT`: diretório raiz que contém `common/compose.base.yaml` (usado se não houver argumento).

A aplicação precisa ser executada em um terminal interativo.

## Estrutura de produtos

Produtos são descobertos dinamicamente em `products/*` (sem recompilar), de forma determinística.

Um diretório de produto é considerado quando contém pelo menos um dos arquivos:

- `apps.json`
- `build.env`
- `.env.example`
- `.env`

Estrutura típica:

```text
products/
  <produto>/
    .env            # gerado/editado pela TUI (não versionar segredos)
    .env.example    # modelo
    apps.json       # opcional, conforme produto
    build.env       # opcional, conforme produto
```

Observações:
- Nomes de diretório inválidos/inseguros (vazio, `.`/`..`, separadores de caminho, caracteres de controle) devem ser evitados.
- Se nenhum produto for encontrado, a TUI entra em modo sem produto configurado (fallback) e indica isso na interface.
- Use a ação **Recarregar produtos** (`L`) para atualizar o catálogo sem reiniciar.

## Ciclo de vida do `.env`

Na view **[3] Bridges & Plugins > Env**:

- **Criar/gerar**: `g` gera a partir de `.env.example` (com confirmação se `.env` já existir; cria backup antes de sobrescrever).
- **Carregar/recarregar**: `r` recarrega o arquivo do disco.
- **Editar**: `↑/↓` seleciona variável, `Enter` edita valor, `Esc` cancela edição.
- **Salvar**: `w` salva de forma atômica (arquivo temporário + rename), cria backup antes de gravar quando aplicável e tenta preservar permissões restritas (`0600` em Unix) e links simbólicos.
- **Revelar/mascarar segredos**: `v`.
- **Gerar segredo**: `n` para chaves elegíveis.
- **Resetar**: `R` abre modal de reset com opções:
  - Restaurar do `.env.example`
  - Remover o `.env`
  - Restaurar backup (`.env.bak.<timestamp>`)
  - Cancelar

O reset exige confirmação digitando o nome do produto (ou frase aceita pela UI), cria backup antes de alterações destrutivas quando aplicável, afeta **apenas** o `.env` do produto selecionado e **não** remove volumes nem altera outras stacks/produtos.

Alterações não salvas: a TUI solicita confirmação ao trocar de produto (`p`) ou sair (`q`) quando há mudanças pendentes no `.env`.

## Doctor / Pre-flight

Na view **[2] Compose Stacks > Operações**:

- `f` executa Pre-flight (validações locais e resumo de prontidão)
- `k` executa Doctor (diagnóstico)
- `K` aplica correções automáticas seguras disponíveis

O Doctor/validações cobrem (conforme implementação atual), entre outros:
- `.env` ausente, `.env.example` ausente
- drift de chaves (ausentes/extras em relação ao exemplo)
- campos obrigatórios/vazios (ex.: `IMAGE_REF`, `DB_PASSWORD`, `TUNNEL_TOKEN`, rede)
- placeholders e formatos suspeitos/inválidos em alguns campos
- validações de build do produto (`build.env`/`apps.json`) quando aplicável
- permissões de `.env` (Unix) e preservação de symlinks em gravações seguras

Correções automáticas são idempotentes quando possível, não inventam segredos e reportam falhas sem marcar como aplicado o que não foi persistido.

## Ações da TUI (principais)

- `1/2/3`: alterna views (Cluster & FinOps, Compose Stacks, Bridges & Plugins)
- `Tab`: alterna painéis/contexto
- `p`: troca produto selecionado
- `L`: recarrega catálogo de produtos
- `q`: sair (com confirmação se houver alterações não salvas)

### Compose Stacks > Operações
- `u`: subir (`common/dc.sh <produto> up -d`)
- `x`: parar (`common/dc.sh <produto> down`, com confirmação)
- `t`: reiniciar (`common/dc.sh <produto> restart`)
- `c`: consultar status (`common/dc.sh <produto> ps`)
- `o`: logs (`common/dc.sh <produto> logs --tail=80 --no-color`)
- `m`: migrar (`bench --site all migrate` via serviço `ops`)
- `f`: pre-flight
- `k`: Doctor
- `K`: aplicar correções automáticas do Doctor

Operações Docker são executadas fora do loop de renderização e exibem saída incremental. A TUI não executa comandos destrutivos de volume automaticamente.

## GUI desktop Electron/React

O frontend desktop fica em `gui/` e usa o mesmo workspace `products/*` e o mesmo wrapper `common/dc.sh` da TUI. Ele não inicia Docker automaticamente, mascara segredos por padrão e confirma operações destrutivas. A remoção de produto apaga somente `products/<id>`; volumes Docker são preservados.

Para executar em desenvolvimento:

```bash
cd gui
npm install
npm run dev
```

Para gerar o bundle do renderer ou um pacote desktop:

```bash
npm run build
npm run dist
```

O fluxo completo de criação de produto continua na TUI; após criar ou alterar produtos, use **Recarregar** na GUI.

## Telemetria Docker

- Coleta em worker de background (fora do tick de renderização), com cache do último snapshot válido.
- Em falhas transitórias, o último snapshot válido é preservado e o erro é registrado separadamente.
- Quando Docker está indisponível, a UI deve continuar responsiva e exibir estado indisponível/sem dados conforme contexto.
- A coleta usa comandos Docker com timeout e backoff em indisponibilidade.

## Perfil opcional de observabilidade

O perfil de observabilidade é **desativado por padrão**. Para ativar por produto:

```bash
common/dc.sh <produto> --observability up -d
common/dc.sh <produto> --observability ps
common/dc.sh <produto> --observability logs
common/dc.sh <produto> --observability down
```

Arquivos relevantes:
- `common/observability.compose.yaml`
- `observability/otel-collector-config.yaml`
- `observability/prometheus.yml`
- `observability/grafana/provisioning/...`

Veja `observability/README.md` para portas, variáveis (`GRAFANA_ADMIN_USER`, `GRAFANA_ADMIN_PASSWORD`), retenção e estado atual das métricas (infraestrutura OTLP/Prometheus/Grafana disponível; a aplicação ainda não emite métricas de negócio por padrão).

## Backups e reset

- Backups de `.env` são criados como `.env.bak.<timestamp>` no diretório do produto em operações relevantes (salvar/sobrescrever/reset/restaurar, conforme fluxo).
- O reset não apaga `.env.example`, não remove volumes e não altera outros produtos.

## Segurança e versionamento

- **Não versione** arquivos `.env` nem credenciais reais.
- Revise permissões do `.env` (recomendado `0600` em Unix).
- A TUI mascara segredos por padrão na visualização.

## Limitações conhecidas

- Algumas verificações Docker do pre-flight/doctor podem depender do ambiente local e da disponibilidade do daemon; a interface deve continuar utilizável sem Docker.
- Comparação visual avançada/importação seletiva/exportação de relatório sem segredos e suporte completo a todas as classes de validação descritas podem estar parcialmente implementados nesta versão.
- Cancelamento de operações Docker em andamento e prevenção total de operações conflitantes simultâneas podem ter cobertura parcial.
- O perfil de observabilidade expõe portas locais quando ativado (por design do perfil); mantenha desativado quando não necessário.

## Validação

Sem subir/parar containers:

```bash
cargo fmt --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
bash -n common/dc.sh
git diff --check
```

Se Docker estiver disponível (somente validação de configuração):

```bash
docker compose -f common/compose.base.yaml -f common/observability.compose.yaml config --quiet
```

Verificações úteis:

```bash
grep -RInE 'Product::(Accounting|Trivia)|\["accounting", "trivia"\]' src common README.md || true
git status --short
git diff --stat
```
