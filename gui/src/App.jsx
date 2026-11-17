import { useEffect, useMemo, useState } from "react";
import "./styles.css";

const api = window.deployer;
const secret = /(PASSWORD|TOKEN|SECRET|KEY)/i;
const nav = ["overview", "operations", "environment", "lifecycle"];

function App() {
  const [data, setData] = useState({ products: [] });
  const [active, setActive] = useState("");
  const [view, setView] = useState("overview");
  const [kind, setKind] = useState("env");
  const [reveal, setReveal] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const product =
    data.products.find((p) => p.id === active) || data.products[0];

  async function refresh() {
    setBusy(true);
    try {
      const next = await api.snapshot();
      setData(next);
      setActive((current) =>
        next.products.some((p) => p.id === current)
          ? current
          : next.products[0]?.id || "",
      );
    } catch (e) {
      setNotice(e.message);
    } finally {
      setBusy(false);
    }
  }
  useEffect(() => {
    refresh();
  }, []);
  const env = product
    ? kind === "build"
      ? product.buildEnv
      : product.env
    : [];
  const errors = useMemo(
    () =>
      product
        ? product.env.filter(
            (x) =>
              x.key &&
              ["IMAGE_REF", "DB_PASSWORD", "TUNNEL_TOKEN"].includes(x.key) &&
              !x.value,
          ).length
        : 0,
    [product],
  );

  async function operation(action) {
    if (!product) return;
    setBusy(true);
    setNotice(`Executando ${action}…`);
    const result = await api.operation({ id: product.id, action });
    setNotice(result.output || (result.ok ? "Concluído." : "Falhou."));
    if (action === "status" || action === "up" || action === "down")
      await refresh();
    setBusy(false);
  }
  async function save(lines) {
    setBusy(true);
    const next = await api.saveEnv({ id: product.id, kind, lines });
    setData(next);
    setNotice(`${kind === "build" ? "build.env" : ".env"} salvo com backup.`);
    setBusy(false);
  }
  async function remove() {
    if (!product) return;
    const next = await api.removeProduct(product.id);
    if (next.products) {
      setData(next);
      setActive(next.products[0]?.id || "");
      setNotice("Produto removido; volumes preservados.");
    }
  }

  return (
    <div className="shell">
      <header className="topbar">
        <div>
          <div className="eyebrow">FRAPPE DEPLOYER</div>
          <h1>Operations cockpit</h1>
          <p>Docker Compose, FinOps e configuração de produtos</p>
        </div>
        <div className="health">
          <span className={data.docker?.ok ? "dot ok" : "dot bad"} /> Docker{" "}
          {data.docker?.ok ? "disponível" : "indisponível"}
          <span className="sep" />
          <span className={data.compose?.ok ? "dot ok" : "dot bad"} /> Compose
        </div>
      </header>
      <div className="toolbar">
        <div className="product-picker">
          <span>PRODUTO ATIVO</span>
          <select value={active} onChange={(e) => setActive(e.target.value)}>
            {data.products.map((p) => (
              <option key={p.id}>{p.id}</option>
            ))}
          </select>
        </div>
        <nav>
          {nav.map((item) => (
            <button
              className={view === item ? "active" : ""}
              onClick={() => setView(item)}
              key={item}
            >
              {item === "overview"
                ? "Overview"
                : item === "operations"
                  ? "Operations"
                  : item === "environment"
                    ? "Bridges & Plugins"
                    : "Life Cycle"}
            </button>
          ))}
        </nav>
        <button className="ghost" onClick={refresh}>
          {busy ? "Atualizando…" : "Recarregar"}
        </button>
      </div>
      {notice && <div className="notice">{notice}</div>}
      <main>
        {!product ? (
          <Empty onCreate={() => setView("lifecycle")} />
        ) : view === "overview" ? (
          <Overview
            product={product}
            errors={errors}
            onOperations={() => setView("operations")}
          />
        ) : view === "operations" ? (
          <Operations product={product} onRun={operation} />
        ) : view === "environment" ? (
          <Environment
            product={product}
            kind={kind}
            setKind={setKind}
            reveal={reveal}
            setReveal={setReveal}
            onSave={save}
          />
        ) : (
          <Lifecycle
            products={data.products}
            active={active}
            select={setActive}
            onRemove={remove}
          />
        )}
      </main>
      <footer>
        <span>Workspace: {data.root || "—"}</span>
        <span>
          Segredos mascarados por padrão · operações destrutivas pedem
          confirmação
        </span>
      </footer>
    </div>
  );
}
function Overview({ product, errors, onOperations }) {
  const configured = product.env.filter((x) => x.key && x.value).length;
  return (
    <section className="grid overview">
      <div className="hero card">
        <div className="eyebrow">CONTROL CENTER</div>
        <h2>{product.id}</h2>
        <p>Estado de configuração e prontidão operacional.</p>
        <div className="metrics">
          <Metric
            label="Variáveis"
            value={configured}
            detail={`de ${product.env.filter((x) => x.key).length}`}
          />
          <Metric
            label="Apps bench"
            value={product.apps.length}
            detail="repositórios"
          />
          <Metric
            label="Alertas"
            value={errors}
            detail={errors ? "requer atenção" : "sem bloqueios"}
            danger={errors > 0}
          />
        </div>
      </div>
      <div className="card checklist">
        <h3>Readiness</h3>
        <Check ok={product.hasExample} text=".env.example encontrado" />
        <Check ok={product.hasEnv} text=".env disponível" />
        <Check ok={errors === 0} text="Variáveis essenciais preenchidas" />
        <button className="primary" onClick={onOperations}>
          Abrir operações
        </button>
      </div>
      <div className="card span-2">
        <h3>Produtos no workspace</h3>
        <p>
          O catálogo é descoberto dinamicamente em <code>products/*</code>. A
          GUI usa o mesmo layout de configuração e os mesmos comandos da TUI.
        </p>
      </div>
    </section>
  );
}
function Operations({ product, onRun }) {
  const actions = [
    ["up", "Subir stack", "docker compose up -d"],
    ["down", "Parar stack", "docker compose down"],
    ["restart", "Reiniciar", "docker compose restart"],
    ["status", "Consultar status", "docker compose ps"],
    ["logs", "Ver logs", "últimas 80 linhas"],
  ];
  return (
    <section className="grid operations">
      <div className="card span-2">
        <div className="section-heading">
          <div>
            <div className="eyebrow">COMPOSE STACK</div>
            <h2>{product.id}</h2>
          </div>
          <span className="pill">sem execução automática</span>
        </div>
        <p>
          As ações são executadas por <code>common/dc.sh</code>; o frontend não
          remove volumes.
        </p>
        <div className="action-grid">
          {actions.map(([id, title, desc]) => (
            <button
              key={id}
              className={id === "down" ? "action danger" : "action"}
              onClick={() => onRun(id)}
            >
              <strong>{title}</strong>
              <small>{desc}</small>
            </button>
          ))}
        </div>
      </div>
      <div className="card output">
        <h3>Última execução</h3>
        <p>
          Os resultados aparecem aqui após uma operação. Use status para
          consultar o estado real sem alterar a stack.
        </p>
      </div>
    </section>
  );
}
function Environment({ product, kind, setKind, reveal, setReveal, onSave }) {
  const source = kind === "build" ? product.buildEnv : product.env;
  const [lines, setLines] = useState(source);
  useEffect(() => setLines(source), [product.id, kind]);
  function update(index, value) {
    setLines((items) =>
      items.map((x, i) => (i === index ? { ...x, value } : x)),
    );
  }
  return (
    <section className="card env">
      <div className="section-heading">
        <div>
          <div className="eyebrow">BRIDGES & PLUGINS</div>
          <h2>{kind === "build" ? "build.env" : ".env"}</h2>
        </div>
        <div className="tabs">
          <button
            className={kind === "env" ? "active" : ""}
            onClick={() => setKind("env")}
          >
            Runtime
          </button>
          <button
            className={kind === "build" ? "active" : ""}
            onClick={() => setKind("build")}
          >
            Build
          </button>
        </div>
      </div>
      <div className="env-actions">
        <button className="ghost" onClick={() => setReveal(!reveal)}>
          {reveal ? "Mascarar segredos" : "Revelar segredos"}
        </button>
        <button className="primary" onClick={() => onSave(lines)}>
          Salvar com backup
        </button>
      </div>
      <div className="env-table">
        {lines
          .filter((x) => x.key)
          .map((line) => (
            <label key={line.index}>
              <span>{line.key}</span>
              <input
                type={secret.test(line.key) && !reveal ? "password" : "text"}
                value={line.value || ""}
                onChange={(e) => update(line.index, e.target.value)}
                placeholder="vazio"
              />
            </label>
          ))}
      </div>
    </section>
  );
}
function Lifecycle({ products, active, select, onRemove }) {
  return (
    <section className="grid lifecycle">
      <div className="card span-2">
        <div className="section-heading">
          <div>
            <div className="eyebrow">PRODUCT LIFECYCLE</div>
            <h2>Catálogo de produtos</h2>
          </div>
          <span className="pill">{products.length} encontrados</span>
        </div>
        <div className="product-list">
          {products.map((p) => (
            <button
              className={
                p.id === active ? "product-row selected" : "product-row"
              }
              key={p.id}
              onClick={() => select(p.id)}
            >
              <span className="status-ring" /> <strong>{p.id}</strong>
              <span>
                {p.hasEnv ? ".env pronto" : "sem .env"} · {p.apps.length} apps
              </span>
            </button>
          ))}
        </div>
        <div className="row-actions">
          <button
            className="ghost"
            onClick={() =>
              alert(
                "Criação de produto usa a modal completa da TUI. Execute a TUI para criar e volte a recarregar a GUI.",
              )
            }
          >
            Novo produto
          </button>
          <button
            className="danger-button"
            onClick={onRemove}
            disabled={!active || products.length < 2}
          >
            Remover selecionado
          </button>
        </div>
      </div>
    </section>
  );
}
function Empty({ onCreate }) {
  return (
    <section className="empty card">
      <h2>Nenhum produto configurado</h2>
      <p>
        Crie um produto pela TUI ou adicione um diretório em{" "}
        <code>products/</code>.
      </p>
      <button className="primary" onClick={onCreate}>
        Abrir Life Cycle
      </button>
    </section>
  );
}
function Metric({ label, value, detail, danger }) {
  return (
    <div className={danger ? "metric danger" : "metric"}>
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{detail}</small>
    </div>
  );
}
function Check({ ok, text }) {
  return (
    <div className="check">
      <span className={ok ? "check-icon ok" : "check-icon"}>
        {ok ? "✓" : "!"}
      </span>
      {text}
    </div>
  );
}
export default App;
