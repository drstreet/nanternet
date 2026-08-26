export type Level = "unknown" | "ok" | "warn" | "critical";

export type Language = "en" | "fa";

export interface Finding {
  code: string;
  level: Level;
  detail: string;
}

export interface WebsiteSpec {
  kind: "website";
  url: string;
  externalProbe: boolean;
  externalIntervalSecs: number;
  expectedStatus: number | null;
  expectedBody: string | null;
  iranNodes: number;
  abroadNodes: number;
}

export type SshAuth = { method: "key"; path: string } | { method: "password" };

export interface ServerSpec {
  kind: "server";
  host: string;
  port: number;
  username: string;
  auth: SshAuth;
  hostFingerprint: string | null;
  loadLimit: number;
  memoryLimit: number;
  diskLimit: number;
  mounts: string[];
  containers: string[];
}

export interface DomainSpec {
  kind: "domain";
  domain: string;
  port: number;
  checkCertificate: boolean;
  checkRegistration: boolean;
  warnDays: number;
}

export type TargetSpec = WebsiteSpec | ServerSpec | DomainSpec;

export type TargetKind = TargetSpec["kind"];

export interface Target {
  id: string;
  name: string;
  enabled: boolean;
  intervalSecs: number;
  spec: TargetSpec;
}

export interface TargetView extends Target {
  secretStored: boolean;
}

export interface LocalProbe {
  reachable: boolean;
  status: number | null;
  latencyMs: number | null;
  error: string | null;
  contentMatch: boolean | null;
}

export interface NodeResult {
  node: string;
  country: string;
  city: string;
  asn: string;
  reachable: boolean | null;
  latencyMs: number | null;
  status: string | null;
  message: string | null;
  address: string | null;
}

export interface ExternalProbe {
  checkedAt: number;
  permanentLink: string | null;
  nodes: NodeResult[];
}

export interface WebsiteReport {
  local: LocalProbe;
  external: ExternalProbe | null;
  externalError: string | null;
}

export interface Usage {
  total: number;
  used: number;
  percent: number;
}

export interface DiskUsage extends Usage {
  mount: string;
}

export interface ContainerState {
  name: string;
  state: string;
  status: string;
  health: string | null;
}

export interface ServerReport {
  hostFingerprint: string | null;
  loadAverage: number | null;
  cores: number | null;
  loadPerCore: number | null;
  uptimeSecs: number | null;
  memory: Usage | null;
  disks: DiskUsage[];
  containers: ContainerState[];
}

export interface CertificateInfo {
  subject: string;
  issuer: string;
  notAfter: number;
  daysLeft: number;
}

export interface RegistrationInfo {
  source: string;
  expiresAt: number;
  daysLeft: number;
}

export interface DomainReport {
  certificate: CertificateInfo | null;
  registration: RegistrationInfo | null;
  registrationError: string | null;
}

export interface TargetStatus {
  targetId: string;
  level: Level;
  checkedAt: number;
  durationMs: number;
  findings: Finding[];
  website?: WebsiteReport;
  server?: ServerReport;
  domain?: DomainReport;
}

export interface Settings {
  language: Language;
  desktopNotifications: boolean;
  discordWebhook: string | null;
  telegramChatId: string | null;
  confirmations: number;
  startAtLogin: boolean;
}

export interface SettingsView extends Settings {
  telegramTokenStored: boolean;
}
