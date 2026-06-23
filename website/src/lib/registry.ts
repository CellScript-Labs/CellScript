import registryDataJson from "../data/registry-packages.json";

export interface RegistryVersion {
  version: string;
  tag: string;
  source_hash: string;
  cellscript_version?: string;
  license?: string;
  released_at?: string;
  yanked?: boolean;
  yanked_at?: string;
  yanked_reason?: string;
  replaced_by?: string;
  abi_index?: string;
  schema_hash?: string;
  dependencies?: Record<string, { namespace: string; version: string }>;
}

export interface RegistryDeployment {
  name?: string;
  status?: string;
  network?: string;
  chain_id?: string;
  out_point?: string;
  code_hash?: string;
  data_hash?: string;
  tx_hash?: string;
  compiler_version?: string;
  artifact_hash?: string;
  metadata_hash?: string;
  schema_hash?: string;
  abi_hash?: string;
  constraints_hash?: string;
}

export interface RegistryPackage {
  coordinate: string;
  namespace: string;
  name: string;
  path: string;
  registry_path: string;
  source_revision?: string;
  description?: string;
  license?: string;
  repository?: string;
  homepage?: string;
  documentation?: string;
  keywords?: string[];
  categories?: string[];
  production?: boolean;
  metadata?: Record<string, unknown>;
  latest_version?: string;
  latest?: RegistryVersion | null;
  versions: RegistryVersion[];
  deployment: {
    count: number;
    active_count: number;
    networks: string[];
    active: RegistryDeployment[];
  };
  status: string;
  install_command: string;
  package_command_prefix: string;
  verify_command: string;
  publish_command: string;
  publish_dry_run_command: string;
  edit_command: string;
}

export interface RegistryData {
  schema_version: number;
  source: string;
  packages: RegistryPackage[];
}

export const registryData = registryDataJson as RegistryData;
export const registryPackages = registryData.packages;

export const registryStats = {
  packages: registryPackages.length,
  namespaces: new Set(registryPackages.map((pkg) => pkg.namespace)).size,
  activeDeployments: registryPackages.reduce((count, pkg) => count + pkg.deployment.active_count, 0),
  productionPackages: registryPackages.filter((pkg) => pkg.production).length,
};

export interface RegistrySection {
  href: string;
  label: string;
  i18nKey: string;
  soon?: boolean;
  soonLabel?: string;
  soonI18nKey?: string;
}

export const registrySections: RegistrySection[] = [
  // The public index route owns the current phase state; keep the shared
  // sub-navigation named consistently across registry pages.
  { href: "/registry", label: "Registry", i18nKey: "registry.nav.browse" },
  { href: "/registry/submit", label: "Submit", i18nKey: "registry.nav.submit" },
  { href: "/registry/manage", label: "Manage", i18nKey: "registry.nav.manage" },
];

export function packageHref(pkg: RegistryPackage): string {
  return `/registry/package/${encodeURIComponent(pkg.namespace)}/${encodeURIComponent(pkg.name)}`;
}

export function findPackage(namespace: string, name: string): RegistryPackage | undefined {
  return registryPackages.find((pkg) => pkg.namespace === namespace && pkg.name === name);
}

export function shortHash(value?: string, visible = 12): string {
  if (!value) return "not recorded";
  const clean = value.startsWith("0x") ? value.slice(2) : value;
  if (clean.length <= visible * 2) return value;
  return `${value.startsWith("0x") ? "0x" : ""}${clean.slice(0, visible)}...${clean.slice(-visible)}`;
}

export function packageSearchText(pkg: RegistryPackage): string {
  return [
    pkg.coordinate,
    pkg.description,
    pkg.latest_version,
    pkg.status,
    pkg.license,
    pkg.path,
    pkg.repository,
    ...(pkg.keywords ?? []),
    ...(pkg.categories ?? []),
    ...pkg.deployment.networks,
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
}
