import { useCallback, useEffect, useState } from "react";
import {
  ArrowClockwise,
  CaretRight,
  CheckCircle,
  CircleNotch,
  Cloud,
  Code,
  Cube,
  Database,
  Eye,
  EyeSlash,
  GearSix,
  List,
  Play,
  Power,
  ShieldCheck,
  TerminalWindow,
  Trash,
  Warning,
  X,
} from "@phosphor-icons/react";
import {
  Button,
  Dialog,
  DialogTrigger,
  Heading,
  Input,
  Label,
  ListBox,
  ListBoxItem,
  Modal,
  Popover,
  Select,
  TextField,
} from "react-aria-components";
import "./App.css";

type View = "overview" | "operations" | "environment" | "lifecycle";
type EnvKind = "env" | "build";
type OperationAction = "up" | "status" | "restart" | "logs" | "migrate" | "down";

interface EnvLine {
  index: number;
  key: string;
  value: string;
  line: string;
}

interface EventItem {
  id: string;
  level: "info" | "success" | "warning" | "error";
  message: string;
  time: string;
}

interface Product {
  id: string;
  apps: string[];
  env: EnvLine[];
  buildEnv: EnvLine[];
  hasExample: boolean;
  hasEnv: boolean;
}

interface Snapshot {
  root: string;
  products: Product[];
  docker: { ok: boolean; text?: string };
  compose: { ok: boolean; text?: string };
  events: EventItem[];
}

const demoSnapshot: Snapshot = {
  root: "/home/frappe/frappe-bench",
  products: [
    {
      id: "demo",
      apps: ["frappe"],
      env: [
        { index: 0, key: "DB_PASSWORD", value: "••••••••", line: "DB_PASSWORD=••••••••" },
      ],
      buildEnv: [],
      hasExample: true,
      hasEnv: true,
    },
  ],
  docker: { ok: true, text: "Docker disponível" },
  compose: { ok: true, text: "Compose disponível" },
  events: [],
};

function createAppClient() {
  const baseUrl = (
    import.meta as ImportMeta & { env?: { VITE_API_BASE_URL?: string } }
  ).env?.VITE_API_BASE_URL?.replace(/\/$/, "") ?? "";

  async function request<T>(path: string, init?: RequestInit): Promise<T> {
    if (!baseUrl) throw new Error("VITE_API_BASE_URL não configurada");
    const response = await fetch(`${baseUrl}${path}`, {
      ...init,
      headers: { "Content-Type": "application/json", ...init?.headers },
    });
    if (!response.ok) throw new Error(`API HTTP ${response.status}`);
    return (await response.json()) as T;
  }

  return {
    snapshot: () => request<Snapshot>("/snapshot"),
    operation: (product: string, action: OperationAction) =>
      request<{ output?: string }>(`/products/${encodeURIComponent(product)}/operations`, {
        method: "POST",
        body: JSON.stringify({ action }),
      }),
    saveEnv: (product: string, kind: EnvKind, lines: EnvLine[]) =>
      request<Snapshot>(`/products/${encodeURIComponent(product)}/environment`, {
        method: "PUT",
        body: JSON.stringify({ kind, lines }),
      }),
    removeProduct: (product: string) =>
      request<Snapshot>(`/products/${encodeURIComponent(product)}`, {
        method: "DELETE",
      }),
  };
}

const client = createAppClient();
const sensitive = /(PASSWORD|TOKEN|SECRET|KEY)/i;

function App() {
  const [snapshot, setSnapshot] = useState<Snapshot>(demoSnapshot);
  const [view, setView] = useState<View>("overview");
  const [activeId, setActiveId] = useState(demoSnapshot.products[0].id);
  const [kind, setKind] = useState<EnvKind>("env");
  const [loading, setLoading] = useState(false);
  const [connected, setConnected] = useState(false);
  const [notice, setNotice] = useState(
    "Modo demonstração: configure VITE_API_BASE_URL para conectar ao app.",
  );
  const product =
    snapshot.products.find((item) => item.id === activeId) ??
    snapshot.products[0];

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const next = await client.snapshot();
      setSnapshot(next);
      setConnected(true);
      setNotice("Conectado ao app. Snapshot atualizado agora.");
      if (!next.products.some((item) => item.id === activeId))
        setActiveId(next.products[0]?.id ?? "");
    } catch (error) {
      setConnected(false);
      setNotice(
        `API indisponível — exibindo dados de demonstração. ${error instanceof Error ? error.message : ""}`,
      );
    } finally {
      setLoading(false);
    }
  }, [activeId]);

  useEffect(() => {
    const timer = window.setTimeout(() => void refresh(), 0);
    return () => window.clearTimeout(timer);
  }, [refresh]);

  async function runOperation(action: OperationAction) {
    if (!product) return;
    setLoading(true);
    setNotice(`Executando ${action} em ${product.id}…`);
    try {
      const result = await client.operation(product.id, action);
      setNotice(result.output || "Operação concluída.");
      await refresh();
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "Falha na operação.");
    } finally {
      setLoading(false);
    }
  }

  async function saveEnv(lines: EnvLine[]) {
    if (!product) return;
    setLoading(true);
    try {
      const next = await client.saveEnv(product.id, kind, lines);
      setSnapshot(next);
      setConnected(true);
      setNotice(`${kind === "env" ? ".env" : "build.env"} salvo com backup.`);
    } catch (error) {
      setNotice(
        error instanceof Error ? error.message : "Falha ao salvar ambiente.",
      );
    } finally {
      setLoading(false);
    }
  }

  async function removeProduct() {
    if (!product) return;
    setLoading(true);
    try {
      const next = await client.removeProduct(product.id);
      setSnapshot(next);
      setActiveId(next.products[0]?.id ?? "");
      setNotice("Produto removido. Volumes Docker permanecem preservados.");
    } catch (error) {
      setNotice(
        error instanceof Error ? error.message : "Falha ao remover produto.",
      );
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">
            <TerminalWindow size={21} weight="duotone" />
          </span>
          <div>
            <strong>frappe</strong>
            <span>deployer</span>
          </div>
        </div>
        <div className="workspace-label">WORKSPACE</div>
        <div className="workspace-path" title={snapshot.root}>
          {snapshot.root}
        </div>
        <nav className="nav" aria-label="Navegação principal">
          <NavButton
            icon={<Cloud />}
            label="Overview"
            active={view === "overview"}
            onPress={() => setView("overview")}
          />
          <NavButton
            icon={<Play />}
            label="Operações"
            active={view === "operations"}
            onPress={() => setView("operations")}
          />
          <NavButton
            icon={<Code />}
            label="Ambiente"
            active={view === "environment"}
            onPress={() => setView("environment")}
          />
          <NavButton
            icon={<List />}
            label="Life Cycle"
            active={view === "lifecycle"}
            onPress={() => setView("lifecycle")}
          />
        </nav>
        <div className="sidebar-bottom">
          <div className="connection">
            <span className={connected ? "status-dot live" : "status-dot"} />
            {connected ? "Conectado ao app" : "Modo demonstração"}
          </div>
          <div className="sidebar-note">
            Segredos mascarados por padrão
            <br />
            Volumes nunca são removidos pela UI
          </div>
        </div>
      </aside>
      <main className="main-content">
        <header className="topbar">
          <div>
            <div className="eyebrow">
              OPERATIONS CONSOLE / {view.toUpperCase()}
            </div>
            <h1>
              {view === "overview"
                ? "Visão geral"
                : view === "operations"
                  ? "Operações da stack"
                  : view === "environment"
                    ? "Bridges & plugins"
                    : "Ciclo de vida"}
            </h1>
          </div>
          <div className="top-actions">
            <Select
              aria-label="Produto ativo"
              selectedKey={activeId}
              onSelectionChange={(key) => setActiveId(String(key))}
              className="product-select"
            >
              <Button>
                <Cube size={17} />
                {product?.id ?? "Sem produto"}
                <CaretRight size={14} />
              </Button>
              <Popover>
                <ListBox>
                  {snapshot.products.map((item) => (
                    <ListBoxItem key={item.id} id={item.id}>
                      {item.id}
                    </ListBoxItem>
                  ))}
                </ListBox>
              </Popover>
            </Select>
            <Button
              className="icon-button"
              aria-label="Atualizar snapshot"
              onPress={() => void refresh()}
            >
              {loading ? <CircleNotch className="spin" /> : <ArrowClockwise />}
            </Button>
          </div>
        </header>
        <div className="notice">
          <span className={connected ? "notice-dot success" : "notice-dot"} />
          {notice}
        </div>
        {view === "overview" && (
          <Overview
            snapshot={snapshot}
            product={product}
            onOperations={() => setView("operations")}
          />
        )}
        {view === "operations" && (
          <Operations
            product={product}
            loading={loading}
            onRun={runOperation}
          />
        )}
        {view === "environment" && (
          <Environment
            key={`${activeId}-${kind}`}
            product={product}
            kind={kind}
            onKindChange={setKind}
            onSave={saveEnv}
          />
        )}
        {view === "lifecycle" && (
          <Lifecycle
            snapshot={snapshot}
            activeId={activeId}
            onSelect={setActiveId}
            onRemove={removeProduct}
          />
        )}
      </main>
    </div>
  );
}

function NavButton({
  icon,
  label,
  active,
  onPress,
}: {
  icon: React.ReactNode;
  label: string;
  active: boolean;
  onPress: () => void;
}) {
  return (
    <Button className={`nav-item ${active ? "active" : ""}`} onPress={onPress}>
      {icon}
      <span>{label}</span>
      {active && <CaretRight className="nav-caret" />}
    </Button>
  );
}
function Overview({
  snapshot,
  product,
  onOperations,
}: {
  snapshot: Snapshot;
  product?: Product;
  onOperations: () => void;
}) {
  const missing =
    product?.env.filter((line) => line.key && !line.value).length ?? 0;
  const events = snapshot.events ?? [];
  return (
    <section className="page">
      <div className="hero-row">
        <div>
          <span className="eyebrow">WORKSPACE HEALTH</span>
          <h2>{missing ? "Atenção necessária" : "Tudo em ordem"}</h2>
          <p className="lede">
            O workspace está pronto para inspeção. Acompanhe estado,
            configuração e operações a partir daqui.
          </p>
        </div>
        <div className={missing ? "health-badge warning" : "health-badge"}>
          <span className="health-ring">
            <ShieldCheck size={25} weight="duotone" />
          </span>
          <div>
            <strong>
              {missing
                ? `${missing} pendência${missing > 1 ? "s" : ""}`
                : "Operacional"}
            </strong>
            <small>última leitura agora</small>
          </div>
        </div>
      </div>
      <div className="metric-grid">
        <Metric
          label="Produtos"
          value={snapshot.products.length}
          detail="no catálogo"
          icon={<Cube />}
        />
        <Metric
          label="Docker"
          value={snapshot.docker.ok ? "Online" : "Offline"}
          detail={snapshot.docker.text ?? "daemon indisponível"}
          icon={<Database />}
          tone={snapshot.docker.ok ? "green" : "red"}
        />
        <Metric
          label="Compose"
          value={snapshot.compose.ok ? "Pronto" : "Indisponível"}
          detail="plugin detectado"
          icon={<GearSix />}
          tone={snapshot.compose.ok ? "green" : "red"}
        />
        <Metric
          label="Ambiente"
          value={missing ? "Revisar" : "Íntegro"}
          detail={product?.id ?? "sem produto"}
          icon={<ShieldCheck />}
          tone={missing ? "amber" : "green"}
        />
      </div>
      <div className="content-grid">
        <section className="panel readiness">
          <PanelTitle
            eyebrow="ATIVO AGORA"
            title={product?.id ?? "Nenhum produto"}
            action={
              <Button className="text-button" onPress={onOperations}>
                Abrir operações <CaretRight />
              </Button>
            }
          />
          <div className="readiness-list">
            <Readiness
              ok={Boolean(product?.hasExample)}
              title="Modelo de ambiente"
              detail={
                product?.hasExample
                  ? ".env.example encontrado"
                  : "modelo ausente"
              }
            />
            <Readiness
              ok={Boolean(product?.hasEnv)}
              title="Runtime configurado"
              detail={
                product?.hasEnv
                  ? ".env disponível"
                  : "gere o .env antes de operar"
              }
            />
            <Readiness
              ok={missing === 0}
              title="Variáveis essenciais"
              detail={
                missing === 0
                  ? "sem campos vazios"
                  : `${missing} campos precisam de atenção`
              }
            />
          </div>
        </section>
        <section className="panel events">
          <PanelTitle
            eyebrow="ACTIVITY FEED"
            title="Eventos recentes"
            action={
              <Button className="text-button" onPress={onOperations}>
                Ver operações <CaretRight />
              </Button>
            }
          />
          {events.map((event) => (
            <Event key={event.id} event={event} />
          ))}
        </section>
      </div>
    </section>
  );
}
function Metric({
  label,
  value,
  detail,
  icon,
  tone = "neutral",
}: {
  label: string;
  value: string | number;
  detail: string;
  icon: React.ReactNode;
  tone?: string;
}) {
  return (
    <div className="metric">
      <div className={`metric-icon ${tone}`}>{icon}</div>
      <div>
        <span>{label}</span>
        <strong>{value}</strong>
        <small>{detail}</small>
      </div>
    </div>
  );
}
function PanelTitle({
  eyebrow,
  title,
  action,
}: {
  eyebrow: string;
  title: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="panel-title">
      <div>
        <span className="eyebrow">{eyebrow}</span>
        <h3>{title}</h3>
      </div>
      {action}
    </div>
  );
}
function Readiness({
  ok,
  title,
  detail,
}: {
  ok: boolean;
  title: string;
  detail: string;
}) {
  return (
    <div className="readiness-item">
      <span className={ok ? "check ok" : "check"}>
        {ok ? <CheckCircle weight="fill" /> : <Warning weight="fill" />}
      </span>
      <div>
        <strong>{title}</strong>
        <span>{detail}</span>
      </div>
      <CaretRight className="muted-icon" />
    </div>
  );
}
function Event({ event }: { event: EventItem }) {
  return (
    <div className="event">
      <span className={`event-mark ${event.level}`} />{" "}
      <span className="event-message">{event.message}</span>
      <time>{event.time}</time>
    </div>
  );
}

function Operations({
  product,
  loading,
  onRun,
}: {
  product?: Product;
  loading: boolean;
  onRun: (action: OperationAction) => void;
}) {
  const actions: Array<[OperationAction, string, string, React.ReactNode]> = [
    ["up", "Subir stack", "Iniciar serviços em segundo plano", <Play />],
    ["status", "Consultar status", "Ler o estado real do Compose", <Cloud />],
    [
      "restart",
      "Reiniciar",
      "Recriar serviços mantendo volumes",
      <ArrowClockwise />,
    ],
    ["logs", "Ver logs", "Abrir as últimas 80 linhas", <TerminalWindow />],
    ["migrate", "Migrar", "Executar migrate pelo serviço ops", <Database />],
  ];
  return (
    <section className="page">
      <div className="section-lead">
        <span className="eyebrow">COMPOSE STACK</span>
        <h2>{product?.id ?? "Sem produto selecionado"}</h2>
        <p>
          O app executa as ações canônicas. O browser não acessa Docker
          diretamente.
        </p>
      </div>
      <div className="operation-grid">
        {actions.map(([id, title, detail, icon]) => (
          <Button
            key={id}
            className="operation-card"
            isDisabled={loading || !product}
            onPress={() => onRun(id)}
          >
            <span className="operation-icon">{icon}</span>
            <span>
              <strong>{title}</strong>
              <small>{detail}</small>
            </span>
            <CaretRight className="operation-arrow" />
          </Button>
        ))}
        <DialogTrigger>
          <Button
            className="operation-card danger-card"
            isDisabled={loading || !product}
          >
            <span className="operation-icon">
              <Power />
            </span>
            <span>
              <strong>Parar stack</strong>
              <small>Não remove volumes Docker</small>
            </span>
            <CaretRight className="operation-arrow" />
          </Button>
          <Modal>
            <Dialog className="dialog">
              {({ close }) => (
                <>
                  <button
                    className="dialog-close"
                    onClick={close}
                    aria-label="Fechar"
                  >
                    <X />
                  </button>
                  <Warning
                    size={30}
                    weight="duotone"
                    className="dialog-warning"
                  />
                  <Heading slot="title">Parar a stack?</Heading>
                  <p>
                    A ação <code>docker compose down</code> será executada para{" "}
                    <strong>{product?.id}</strong>. Os volumes Docker serão
                    preservados.
                  </p>
                  <div className="dialog-actions">
                    <Button className="secondary-button" onPress={close}>
                      Cancelar
                    </Button>
                    <Button
                      className="danger-button"
                      onPress={() => {
                        close();
                        onRun("down");
                      }}
                    >
                      Parar stack
                    </Button>
                  </div>
                </>
              )}
            </Dialog>
          </Modal>
        </DialogTrigger>
      </div>
    </section>
  );
}

function Environment({
  product,
  kind,
  onKindChange,
  onSave,
}: {
  product?: Product;
  kind: EnvKind;
  onKindChange: (kind: EnvKind) => void;
  onSave: (lines: EnvLine[]) => void;
}) {
  const source =
    kind === "env" ? (product?.env ?? []) : (product?.buildEnv ?? []);
  const [lines, setLines] = useState(source);
  const [reveal, setReveal] = useState(false);
  return (
    <section className="page">
      <div className="section-lead">
        <span className="eyebrow">BRIDGES & PLUGINS</span>
        <h2>Configuração de ambiente</h2>
        <p>
          Edite os arquivos do produto com backup e preservação de comentários.
        </p>
      </div>
      <div className="panel env-panel">
        <div className="env-toolbar">
          <div className="segmented">
            <Button
              className={kind === "env" ? "selected" : ""}
              onPress={() => onKindChange("env")}
            >
              Runtime <code>.env</code>
            </Button>
            <Button
              className={kind === "build" ? "selected" : ""}
              onPress={() => onKindChange("build")}
            >
              Build <code>build.env</code>
            </Button>
          </div>
          <div className="env-actions">
            <Button
              className="secondary-button"
              onPress={() => setReveal(!reveal)}
            >
              {reveal ? <EyeSlash /> : <Eye />}
              {reveal ? "Mascarar segredos" : "Revelar segredos"}
            </Button>
            <Button className="primary-button" onPress={() => onSave(lines)}>
              <CheckCircle />
              Salvar com backup
            </Button>
          </div>
        </div>
        <div className="env-fields">
          {lines
            .filter((line) => line.key)
            .map((line) => (
              <TextField key={line.index} className="env-field">
                <Label>{line.key}</Label>
                <Input
                  type={
                    line.key && sensitive.test(line.key) && !reveal
                      ? "password"
                      : "text"
                  }
                  value={line.value ?? ""}
                  onChange={(event) =>
                    setLines((items) =>
                      items.map((item) =>
                        item.index === line.index
                          ? {
                              ...item,
                              value: event.target.value,
                              line: `${item.key}=${event.target.value}`,
                            }
                          : item,
                      ),
                    )
                  }
                />
                <span className="field-index">
                  #{String(line.index + 1).padStart(2, "0")}
                </span>
              </TextField>
            ))}
        </div>
        <p className="env-footnote">
          <ShieldCheck /> Linhas sem chave, comentários e ordem original serão
          preservados pelo app.
        </p>
      </div>
    </section>
  );
}

function Lifecycle({
  snapshot,
  activeId,
  onSelect,
  onRemove,
}: {
  snapshot: Snapshot;
  activeId: string;
  onSelect: (id: string) => void;
  onRemove: () => void;
}) {
  return (
    <section className="page">
      <div className="section-lead">
        <span className="eyebrow">PRODUCT LIFECYCLE</span>
        <h2>Catálogo de produtos</h2>
        <p>Selecione um produto para revisar o ambiente ou operar a stack.</p>
      </div>
      <div className="panel lifecycle-panel">
        <div className="catalog-header">
          <span>{snapshot.products.length} produtos encontrados</span>
          <Button className="secondary-button">
            <GearSix />
            Gerenciar catálogo
          </Button>
        </div>
        {snapshot.products.map((item) => (
          <Button
            key={item.id}
            className={`product-row ${item.id === activeId ? "selected" : ""}`}
            onPress={() => onSelect(item.id)}
          >
            <span
              className={`product-status ${item.hasEnv ? "ready" : "pending"}`}
            />
            <div>
              <strong>{item.id}</strong>
              <span>
                {item.apps.length} apps conectados ·{" "}
                {item.hasEnv ? ".env pronto" : "sem .env"}
              </span>
            </div>
            <CaretRight className="muted-icon" />
          </Button>
        ))}
        <div className="lifecycle-footer">
          <span>
            <ShieldCheck /> Volumes Docker são preservados
          </span>
          <DialogTrigger>
            <Button
              className="danger-button"
              isDisabled={!activeId || snapshot.products.length < 2}
            >
              <Trash /> Remover selecionado
            </Button>
            <Modal>
              <Dialog className="dialog">
                {({ close }) => (
                  <>
                    <button
                      className="dialog-close"
                      onClick={close}
                      aria-label="Fechar"
                    >
                      <X />
                    </button>
                    <Trash
                      size={30}
                      weight="duotone"
                      className="dialog-warning"
                    />
                    <Heading slot="title">Remover {activeId}?</Heading>
                    <p>
                      Somente <code>products/{activeId}</code> será removido.
                      Nenhum volume Docker será tocado.
                    </p>
                    <div className="dialog-actions">
                      <Button className="secondary-button" onPress={close}>
                        Cancelar
                      </Button>
                      <Button
                        className="danger-button"
                        onPress={() => {
                          close();
                          onRemove();
                        }}
                      >
                        Remover produto
                      </Button>
                    </div>
                  </>
                )}
              </Dialog>
            </Modal>
          </DialogTrigger>
        </div>
      </div>
    </section>
  );
}

export default App;
