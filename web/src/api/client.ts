export type EnvKind = "env" | "build";
export type OperationAction =
  | "up"
  | "down"
  | "restart"
  | "status"
  | "logs"
  | "migrate";

export type EnvLine = {
  index: number;
  key: string | null;
  value?: string;
  line: string;
};

export type Product = {
  id: string;
  env: EnvLine[];
  buildEnv: EnvLine[];
  apps: string[];
  hasEnv: boolean;
  hasExample: boolean;
};

export type RuntimeStatus = { ok: boolean; text?: string };
export type EventItem = {
  id: string;
  time: string;
  level: "info" | "success" | "warning" | "error";
  message: string;
};

export type Snapshot = {
  root: string;
  products: Product[];
  docker: RuntimeStatus;
  compose: RuntimeStatus;
  events?: EventItem[];
};

export type ApiError = { message: string; code?: string };
export type ApiEnvelope<T> = { data?: T; error?: ApiError };

type DesktopBridge = {
  snapshot: () => Promise<Snapshot>;
  saveEnv: (payload: {
    id: string;
    kind: EnvKind;
    lines: EnvLine[];
  }) => Promise<Snapshot>;
  operation: (payload: {
    id: string;
    action: OperationAction;
  }) => Promise<{ ok: boolean; output?: string }>;
  removeProduct: (id: string) => Promise<Snapshot>;
};

declare global {
  interface Window {
    deployer?: DesktopBridge;
  }
}

const demoLines = (entries: Array<[string, string]>): EnvLine[] =>
  entries.map(([key, value], index) => ({
    index,
    key,
    value,
    line: `${key}=${value}`,
  }));

export const demoSnapshot: Snapshot = {
  root: "/workspace/musical-lamp",
  docker: { ok: true, text: "Docker Engine 27.5" },
  compose: { ok: true, text: "Docker Compose v2.32" },
  products: [
    {
      id: "accounting",
      hasEnv: true,
      hasExample: true,
      apps: ["erpnext", "payments"],
      env: demoLines([
        ["IMAGE_REF", "frappe/erpnext:v15"],
        ["DB_PASSWORD", "••••••••"],
        ["TUNNEL_TOKEN", "••••••••"],
      ]),
      buildEnv: demoLines([
        ["FRAPPE_BRANCH", "version-15"],
        ["APPS_JSON", "apps.json"],
      ]),
    },
    {
      id: "trivia",
      hasEnv: false,
      hasExample: true,
      apps: ["erpnext"],
      env: demoLines([
        ["IMAGE_REF", ""],
        ["DB_PASSWORD", ""],
        ["TUNNEL_TOKEN", ""],
      ]),
      buildEnv: demoLines([["FRAPPE_BRANCH", "version-15"]]),
    },
  ],
  events: [
    {
      id: "1",
      time: "agora",
      level: "success",
      message: "Snapshot do workspace carregado",
    },
    {
      id: "2",
      time: "há 4 min",
      level: "info",
      message: "Nenhuma operação Docker em andamento",
    },
  ],
};

export function createAppClient(
  baseUrl = import.meta.env.VITE_API_BASE_URL || "",
) {
  const normalized = baseUrl.replace(/\/$/, "");
  const desktop = typeof window !== "undefined" ? window.deployer : undefined;
  const request = async <T>(path: string, init?: RequestInit): Promise<T> => {
    const response = await fetch(`${normalized}${path}`, {
      ...init,
      headers: {
        Accept: "application/json",
        "Content-Type": "application/json",
        "X-Client": "frappe-deployer-web",
        ...init?.headers,
      },
    });
    const body = (await response.json().catch(() => ({}))) as unknown;
    const envelope = body as ApiEnvelope<T>;
    if (!response.ok) {
      const error =
        typeof body === "object" &&
        body !== null &&
        "error" in body &&
        envelope.error
          ? envelope.error.message
          : `O app respondeu ${response.status}.`;
      throw new Error(error);
    }
    return typeof body === "object" &&
      body !== null &&
      "data" in body &&
      envelope.data !== undefined
      ? envelope.data
      : (body as T);
  };

  return {
    async snapshot() {
      return desktop
        ? desktop.snapshot()
        : request<Snapshot>("/api/v1/snapshot");
    },
    async operation(id: string, action: OperationAction) {
      return desktop
        ? desktop.operation({ id, action })
        : request<{ ok: boolean; output?: string }>(
            `/api/v1/products/${encodeURIComponent(id)}/operations`,
            { method: "POST", body: JSON.stringify({ action }) },
          );
    },
    async saveEnv(id: string, kind: EnvKind, lines: EnvLine[]) {
      return desktop
        ? desktop.saveEnv({ id, kind, lines })
        : request<Snapshot>(`/api/v1/products/${encodeURIComponent(id)}/env`, {
            method: "PUT",
            body: JSON.stringify({ kind, lines }),
          });
    },
    async removeProduct(id: string) {
      return desktop
        ? desktop.removeProduct(id)
        : request<Snapshot>(`/api/v1/products/${encodeURIComponent(id)}`, {
            method: "DELETE",
          });
    },
  };
}
