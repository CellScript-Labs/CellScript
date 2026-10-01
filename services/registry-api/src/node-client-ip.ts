import { isIP } from "node:net";

const MAX_TRUSTED_PROXY_HOPS = 16;

export function parseTrustedProxyHops(value: string | undefined): number {
  if (value === undefined || value.trim() === "") {
    return 0;
  }
  if (!/^\d+$/.test(value.trim())) {
    throw new Error("REGISTRY_TRUSTED_PROXY_HOPS must be an integer in 0..16");
  }
  const hops = Number(value);
  if (!Number.isSafeInteger(hops) || hops > MAX_TRUSTED_PROXY_HOPS) {
    throw new Error("REGISTRY_TRUSTED_PROXY_HOPS must be an integer in 0..16");
  }
  return hops;
}

export function resolveNodeClientIp(
  remoteAddress: string | undefined,
  forwardedFor: string | string[] | undefined,
  trustedProxyHops: number,
): string | undefined {
  const peer = normalizeIp(remoteAddress);
  if (trustedProxyHops === 0) {
    return peer;
  }

  const chain = (Array.isArray(forwardedFor) ? forwardedFor.join(",") : forwardedFor ?? "")
    .split(",")
    .map((entry) => normalizeIp(entry.trim()));
  const selectedIndex = chain.length - trustedProxyHops;
  if (selectedIndex < 0 || chain.slice(selectedIndex).some((entry) => entry === undefined)) {
    return peer;
  }
  return chain[selectedIndex] ?? peer;
}

export function applyNodeClientIdentity(
  headers: Headers,
  remoteAddress: string | undefined,
  forwardedFor: string | string[] | undefined,
  trustedProxyHops: number,
): void {
  const clientIp = resolveNodeClientIp(remoteAddress, forwardedFor, trustedProxyHops);
  headers.delete("cf-asn");
  headers.delete("cf-connecting-ip");
  headers.delete("x-forwarded-for");
  if (clientIp) {
    headers.set("cf-connecting-ip", clientIp);
  }
}

function normalizeIp(value: string | undefined): string | undefined {
  const candidate = value?.trim();
  if (!candidate) {
    return undefined;
  }
  const mappedIpv4 = candidate.match(/^::ffff:(\d{1,3}(?:\.\d{1,3}){3})$/i)?.[1];
  if (mappedIpv4 && isIP(mappedIpv4) === 4) {
    return mappedIpv4;
  }
  return isIP(candidate) === 0 ? undefined : candidate.toLowerCase();
}
