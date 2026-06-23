import type { SignChallengeResponseData } from "@joyid/ckb";

export const AUTH_PROTOCOL = "cellscript-registry-auth-v1";
export const AUTH_ACTION = "authorize_capability";
export const AUTH_REVOKE_CAPABILITY_ACTION = "revoke_capability";
export const PUBLISH_PROTOCOL = "cellscript-registry-publish-v1";
export const PUBLISH_ACTION = "publish";
export const DEFAULT_REGISTRY_ORIGIN = "https://api.registry.cellscript.dev";
export const DEFAULT_STATIC_REGISTRY_ORIGIN = "https://registry.cellscript.dev";
export const ACCEPTED_PRINCIPAL_TYPE = "joyid_ckb";
export const JOYID_CKB_PRINCIPAL_BINDING_CONTEXT = "cellscript-registry-joyid-ckb-principal-v1";

export type RegistryEntryStatus =
  | "source_published"
  | "indexed_pending"
  | "verified_build"
  | "deployed"
  | "on_chain_attested"
  | "deprecated"
  | "yanked"
  | "quarantined";

export interface CapabilityAuthorisationPayload {
  protocol: typeof AUTH_PROTOCOL;
  action: typeof AUTH_ACTION;
  registry_origin: string;
  principal_type: typeof ACCEPTED_PRINCIPAL_TYPE;
  principal_id: string;
  capability_pubkey: string;
  requested_scopes: string[];
  capability_expires_at: string;
  nonce: string;
  issued_at: string;
  expires_at: string;
  cli_version: string;
}

export interface CapabilityRevocationPayload {
  protocol: typeof AUTH_PROTOCOL;
  action: typeof AUTH_REVOKE_CAPABILITY_ACTION;
  registry_origin: string;
  principal_type: typeof ACCEPTED_PRINCIPAL_TYPE;
  principal_id: string;
  capability_key_id: string;
  nonce: string;
  issued_at: string;
  expires_at: string;
  cli_version: string;
}

export interface PublishPayload {
  protocol: typeof PUBLISH_PROTOCOL;
  action: typeof PUBLISH_ACTION;
  registry_origin: string;
  namespace: string;
  name: string;
  version: string;
  source_hash: string;
  manifest_hash?: string;
  capability_key_id: string;
  nonce: string;
  issued_at: string;
  expires_at: string;
  cli_version: string;
  registry_entry: Record<string, unknown>;
}

export interface SourceSnapshotInput {
  content_base64: string;
  content_type: string;
  size_bytes: number;
  source_hash: string;
}

export interface CapabilitySignature {
  algorithm: "p256-sha256";
  signature: string;
}

export interface JoyidVerifier {
  verifySignature(signature: SignChallengeResponseData): Promise<boolean>;
}

export interface CapabilitySignatureVerifier {
  verify(canonicalPayload: string, capabilityPubkey: string, signature: CapabilitySignature): Promise<boolean>;
}

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly code: string,
    message: string,
  ) {
    super(message);
  }
}

export function canonicalJson(value: unknown): string {
  return JSON.stringify(sortForJson(value));
}

function sortForJson(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(sortForJson);
  }
  if (value && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(value).sort()) {
      const item = (value as Record<string, unknown>)[key];
      if (item !== undefined) {
        out[key] = sortForJson(item);
      }
    }
    return out;
  }
  return value;
}

export async function sha256Hex(input: string | Uint8Array | ArrayBuffer): Promise<string> {
  const data =
    typeof input === "string" ? new TextEncoder().encode(input) : input instanceof Uint8Array ? input : new Uint8Array(input);
  const hash = await crypto.subtle.digest("SHA-256", toArrayBuffer(data));
  return [...new Uint8Array(hash)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function base64ToBytes(value: string): Uint8Array<ArrayBuffer> {
  const binary = atob(value);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    out[i] = binary.charCodeAt(i);
  }
  return out;
}

export function base64UrlToBytes(value: string): Uint8Array<ArrayBuffer> {
  const base64 = value.replace(/-/g, "+").replace(/_/g, "/").padEnd(Math.ceil(value.length / 4) * 4, "=");
  return base64ToBytes(base64);
}

export function hexToBytes(value: string): Uint8Array<ArrayBuffer> {
  const clean = value.startsWith("0x") ? value.slice(2) : value;
  if (!/^[0-9a-fA-F]*$/.test(clean) || clean.length % 2 !== 0) {
    throw new ApiError(400, "invalid_hex", "hex string is malformed");
  }
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < clean.length; i += 2) {
    out[i / 2] = Number.parseInt(clean.slice(i, i + 2), 16);
  }
  return out;
}

export function parseSignatureBytes(value: string): Uint8Array<ArrayBuffer> {
  return value.startsWith("0x") ? hexToBytes(value) : base64UrlToBytes(value);
}

function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

export function assertPlainObject(value: unknown, code: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new ApiError(400, code, "request body must be a JSON object");
  }
  return value as Record<string, unknown>;
}

export function requireString(value: Record<string, unknown>, key: string): string {
  const item = value[key];
  if (typeof item !== "string" || item.trim() === "") {
    throw new ApiError(400, "invalid_field", `${key} is required`);
  }
  return item.trim();
}

export function requireStringArray(value: Record<string, unknown>, key: string): string[] {
  const item = value[key];
  if (!Array.isArray(item) || item.length === 0 || item.some((entry) => typeof entry !== "string" || entry.trim() === "")) {
    throw new ApiError(400, "invalid_field", `${key} must be a non-empty string array`);
  }
  return item.map((entry) => entry.trim());
}

export function parseTimestamp(value: string, key: string): Date {
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) {
    throw new ApiError(400, "invalid_timestamp", `${key} must be an ISO timestamp`);
  }
  return date;
}

export function validatePackageIdent(value: string, field: string): string {
  const trimmed = value.trim();
  if (!/^[a-z0-9][a-z0-9_-]{1,62}$/.test(trimmed)) {
    throw new ApiError(400, "invalid_package_identifier", `${field} must be lowercase ascii, 2-63 chars`);
  }
  return trimmed;
}

export function validateVersion(value: string): string {
  const trimmed = value.trim();
  if (!/^[0-9]+[.][0-9]+[.][0-9]+(?:[-+][0-9A-Za-z.-]+)?$/.test(trimmed)) {
    throw new ApiError(400, "invalid_version", "version must be semver-like");
  }
  return trimmed;
}

export function validateCapabilityPayload(
  payload: unknown,
  registryOrigin: string,
  now: Date,
): CapabilityAuthorisationPayload {
  const obj = assertPlainObject(payload, "invalid_capability_payload");
  const protocol = requireString(obj, "protocol");
  const action = requireString(obj, "action");
  const principalType = requireString(obj, "principal_type");
  const principalId = requireString(obj, "principal_id");
  const capabilityPubkey = requireString(obj, "capability_pubkey");
  const requestedScopes = requireStringArray(obj, "requested_scopes");
  const capabilityExpiresAt = requireString(obj, "capability_expires_at");
  const nonce = requireString(obj, "nonce");
  const issuedAt = requireString(obj, "issued_at");
  const expiresAt = requireString(obj, "expires_at");
  const cliVersion = requireString(obj, "cli_version");

  if (protocol !== AUTH_PROTOCOL || action !== AUTH_ACTION) {
    throw new ApiError(400, "invalid_auth_action", "capability payload has the wrong protocol or action");
  }
  if (requireString(obj, "registry_origin") !== registryOrigin) {
    throw new ApiError(400, "invalid_registry_origin", "capability payload registry_origin does not match this API");
  }
  if (principalType !== ACCEPTED_PRINCIPAL_TYPE) {
    throw new ApiError(400, "unsupported_principal_type", "only joyid_ckb principals are accepted");
  }
  if (!/^0x[0-9a-fA-F]{40,64}$/.test(principalId) && !/^ck[bt]1[0-9a-z]+$/.test(principalId)) {
    throw new ApiError(400, "invalid_principal_id", "principal_id must be a normalized JoyID/CKB identity binding");
  }
  if (requestedScopes.some((scope) => !/^publish:[a-z0-9][a-z0-9_-]{1,62}\/[a-z0-9][a-z0-9_-]{1,62}$/.test(scope))) {
    throw new ApiError(400, "invalid_scope", "requested_scopes may only contain publish:namespace/package scopes");
  }
  if (!/^0x[0-9a-fA-F]{16,}$/.test(nonce)) {
    throw new ApiError(400, "invalid_nonce", "nonce must be hex and at least 8 bytes");
  }
  const expires = parseTimestamp(expiresAt, "expires_at");
  const capabilityExpires = parseTimestamp(capabilityExpiresAt, "capability_expires_at");
  parseTimestamp(issuedAt, "issued_at");
  if (expires.getTime() <= now.getTime()) {
    throw new ApiError(401, "auth_payload_expired", "capability authorisation challenge has expired");
  }
  if (capabilityExpires.getTime() <= now.getTime()) {
    throw new ApiError(400, "capability_expired", "capability expiry must be in the future");
  }

  return {
    protocol: AUTH_PROTOCOL,
    action: AUTH_ACTION,
    registry_origin: registryOrigin,
    principal_type: ACCEPTED_PRINCIPAL_TYPE,
    principal_id: principalId,
    capability_pubkey: capabilityPubkey,
    requested_scopes: requestedScopes,
    capability_expires_at: capabilityExpiresAt,
    nonce,
    issued_at: issuedAt,
    expires_at: expiresAt,
    cli_version: cliVersion,
  };
}

export function validateCapabilityRevocationPayload(
  payload: unknown,
  registryOrigin: string,
  now: Date,
): CapabilityRevocationPayload {
  const obj = assertPlainObject(payload, "invalid_capability_revocation_payload");
  const protocol = requireString(obj, "protocol");
  const action = requireString(obj, "action");
  const principalType = requireString(obj, "principal_type");
  const principalId = requireString(obj, "principal_id");
  const capabilityKeyId = requireString(obj, "capability_key_id");
  const nonce = requireString(obj, "nonce");
  const issuedAt = requireString(obj, "issued_at");
  const expiresAt = requireString(obj, "expires_at");
  const cliVersion = requireString(obj, "cli_version");

  if (protocol !== AUTH_PROTOCOL || action !== AUTH_REVOKE_CAPABILITY_ACTION) {
    throw new ApiError(400, "invalid_capability_revocation_action", "capability revocation payload has the wrong protocol or action");
  }
  if (requireString(obj, "registry_origin") !== registryOrigin) {
    throw new ApiError(400, "invalid_registry_origin", "capability revocation registry_origin does not match this API");
  }
  if (principalType !== ACCEPTED_PRINCIPAL_TYPE) {
    throw new ApiError(400, "unsupported_principal_type", "only joyid_ckb principals are accepted");
  }
  if (!/^0x[0-9a-fA-F]{40,64}$/.test(principalId) && !/^ck[bt]1[0-9a-z]+$/.test(principalId)) {
    throw new ApiError(400, "invalid_principal_id", "principal_id must be a normalized JoyID/CKB identity binding");
  }
  if (!/^cap_[0-9a-f]{32}$/.test(capabilityKeyId)) {
    throw new ApiError(400, "invalid_capability_key_id", "capability_key_id is malformed");
  }
  if (!/^0x[0-9a-fA-F]{16,}$/.test(nonce)) {
    throw new ApiError(400, "invalid_nonce", "nonce must be hex and at least 8 bytes");
  }
  parseTimestamp(issuedAt, "issued_at");
  if (parseTimestamp(expiresAt, "expires_at").getTime() <= now.getTime()) {
    throw new ApiError(401, "capability_revocation_payload_expired", "capability revocation challenge has expired");
  }

  return {
    protocol: AUTH_PROTOCOL,
    action: AUTH_REVOKE_CAPABILITY_ACTION,
    registry_origin: registryOrigin,
    principal_type: ACCEPTED_PRINCIPAL_TYPE,
    principal_id: principalId,
    capability_key_id: capabilityKeyId,
    nonce,
    issued_at: issuedAt,
    expires_at: expiresAt,
    cli_version: cliVersion,
  };
}

export function validatePublishPayload(payload: unknown, registryOrigin: string, now: Date): PublishPayload {
  const obj = assertPlainObject(payload, "invalid_publish_payload");
  const protocol = requireString(obj, "protocol");
  const action = requireString(obj, "action");
  if (protocol !== PUBLISH_PROTOCOL || action !== PUBLISH_ACTION) {
    throw new ApiError(400, "invalid_publish_action", "publish payload has the wrong protocol or action");
  }
  if (requireString(obj, "registry_origin") !== registryOrigin) {
    throw new ApiError(400, "invalid_registry_origin", "publish payload registry_origin does not match this API");
  }
  const namespace = validatePackageIdent(requireString(obj, "namespace"), "namespace");
  const name = validatePackageIdent(requireString(obj, "name"), "name");
  const version = validateVersion(requireString(obj, "version"));
  const sourceHash = requireString(obj, "source_hash");
  if (!/^([a-z0-9_-]+:)?0x[0-9a-fA-F]{32,128}$/.test(sourceHash) && !/^[0-9a-fA-F]{32,128}$/.test(sourceHash)) {
    throw new ApiError(400, "invalid_source_hash", "source_hash must be a hex content hash");
  }
  const capabilityKeyId = requireString(obj, "capability_key_id");
  const nonce = requireString(obj, "nonce");
  const issuedAt = requireString(obj, "issued_at");
  const expiresAt = requireString(obj, "expires_at");
  const cliVersion = requireString(obj, "cli_version");
  const registryEntry = assertPlainObject(obj["registry_entry"], "invalid_registry_entry");
  parseTimestamp(issuedAt, "issued_at");
  if (parseTimestamp(expiresAt, "expires_at").getTime() <= now.getTime()) {
    throw new ApiError(401, "publish_payload_expired", "publish payload has expired");
  }
  if (!/^0x[0-9a-fA-F]{16,}$/.test(nonce)) {
    throw new ApiError(400, "invalid_nonce", "nonce must be hex and at least 8 bytes");
  }

  const result: PublishPayload = {
    protocol: PUBLISH_PROTOCOL,
    action: PUBLISH_ACTION,
    registry_origin: registryOrigin,
    namespace,
    name,
    version,
    source_hash: sourceHash,
    capability_key_id: capabilityKeyId,
    nonce,
    issued_at: issuedAt,
    expires_at: expiresAt,
    cli_version: cliVersion,
    registry_entry: registryEntry,
  };
  if (typeof obj["manifest_hash"] === "string") {
    result.manifest_hash = obj["manifest_hash"];
  }
  return result;
}

export function validateSnapshot(input: unknown, payload: PublishPayload, maxBytes: number): SourceSnapshotInput {
  const obj = assertPlainObject(input, "invalid_source_snapshot");
  const contentBase64 = requireString(obj, "content_base64");
  const contentType = requireString(obj, "content_type");
  const sizeBytes = Number(obj["size_bytes"]);
  const sourceHash = requireString(obj, "source_hash");
  if (!Number.isInteger(sizeBytes) || sizeBytes <= 0 || sizeBytes > maxBytes) {
    throw new ApiError(413, "snapshot_too_large", `source snapshot must be 1..${maxBytes} bytes`);
  }
  if (sourceHash !== payload.source_hash) {
    throw new ApiError(400, "snapshot_source_hash_mismatch", "snapshot source_hash must match publish payload source_hash");
  }
  return { content_base64: contentBase64, content_type: contentType, size_bytes: sizeBytes, source_hash: sourceHash };
}

export async function verifyJoyidAuthorisationPayload(
  payload: CapabilityAuthorisationPayload,
  signature: SignChallengeResponseData,
  verifier: JoyidVerifier,
): Promise<void> {
  return verifyJoyidPayloadSignature(payload, signature, verifier);
}

export async function verifyJoyidPayloadSignature(
  payload: unknown,
  signature: SignChallengeResponseData,
  verifier: JoyidVerifier,
): Promise<void> {
  const expectedChallenge = canonicalJson(payload);
  if (signature.challenge !== expectedChallenge) {
    throw new ApiError(401, "joyid_challenge_mismatch", "JoyID signature challenge does not match the capability payload");
  }
  if (signature.keyType !== "main_key" && signature.keyType !== "sub_key") {
    throw new ApiError(401, "joyid_root_required", "capability authorisation must be signed by a JoyID main or sub key");
  }
  await verifyJoyidPrincipalBinding(payload, signature);
  if (!(await verifier.verifySignature(signature))) {
    throw new ApiError(401, "joyid_signature_invalid", "JoyID signature verification failed");
  }
}

async function verifyJoyidPrincipalBinding(payload: unknown, signature: SignChallengeResponseData): Promise<void> {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
    return;
  }
  const obj = payload as Record<string, unknown>;
  if (obj["principal_type"] !== ACCEPTED_PRINCIPAL_TYPE || typeof obj["principal_id"] !== "string") {
    return;
  }
  const principalId = obj["principal_id"].trim().toLowerCase();
  const candidates = await joyidPrincipalIdCandidates(signature);
  if (!candidates.includes(principalId)) {
    throw new ApiError(401, "joyid_principal_mismatch", "JoyID signature does not match payload principal_id");
  }
}

export async function joyidPrincipalIdCandidates(signature: Pick<SignChallengeResponseData, "pubkey" | "keyType">): Promise<string[]> {
  if (typeof signature.pubkey !== "string" || signature.pubkey.trim() === "") {
    throw new ApiError(401, "joyid_pubkey_missing", "JoyID signature must include pubkey");
  }
  if (signature.keyType !== "main_key" && signature.keyType !== "sub_key") {
    throw new ApiError(401, "joyid_root_required", "capability authorisation must be signed by a JoyID main or sub key");
  }
  const pubkey = normalizeJoyidPubkey(signature.pubkey);
  const candidates = new Set<string>();
  candidates.add(await joyidPrincipalIdFromBinding(signature.keyType, pubkey));
  if (/^[0-9a-f]{40,64}$/.test(pubkey)) {
    candidates.add(`0x${pubkey}`);
  }
  return [...candidates];
}

export async function joyidPrincipalIdFromBinding(keyType: "main_key" | "sub_key", pubkey: string): Promise<string> {
  const material = `${JOYID_CKB_PRINCIPAL_BINDING_CONTEXT}\n${keyType}\n${normalizeJoyidPubkey(pubkey)}`;
  return `0x${await sha256Hex(material)}`;
}

function normalizeJoyidPubkey(pubkey: string): string {
  const value = pubkey.trim().toLowerCase();
  return value.startsWith("0x") ? value.slice(2) : value;
}

export function scopeAllowsPublish(scopes: string[], namespace: string, name: string): boolean {
  return scopes.includes(`publish:${namespace}/${name}`) || scopes.includes(`publish:${namespace}/*`);
}

export async function capabilityKeyId(capabilityPubkey: string): Promise<string> {
  return `cap_${(await sha256Hex(capabilityPubkey)).slice(0, 32)}`;
}

export class WebCryptoP256Verifier implements CapabilitySignatureVerifier {
  async verify(canonicalPayload: string, capabilityPubkey: string, signature: CapabilitySignature): Promise<boolean> {
    if (signature.algorithm !== "p256-sha256" || !capabilityPubkey.startsWith("p256-spki:")) {
      return false;
    }
    const spki = base64UrlToBytes(capabilityPubkey.slice("p256-spki:".length));
    const sig = parseSignatureBytes(signature.signature);
    const key = await crypto.subtle.importKey(
      "spki",
      toArrayBuffer(spki),
      { name: "ECDSA", namedCurve: "P-256" },
      false,
      ["verify"],
    );
    return crypto.subtle.verify(
      { name: "ECDSA", hash: "SHA-256" },
      key,
      toArrayBuffer(sig),
      new TextEncoder().encode(canonicalPayload),
    );
  }
}
