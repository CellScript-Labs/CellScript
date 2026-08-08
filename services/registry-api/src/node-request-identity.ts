export function trustedClientIp(
  forwardedHeader: string | undefined,
  socketIp: string | undefined,
  trustedProxyHops: number,
): string | undefined {
  const directPeer = socketIp?.trim();
  if (trustedProxyHops === 0) return directPeer || undefined;
  const forwarded = forwardedHeader
    ?.split(",")
    .map((value) => value.trim())
    .filter(Boolean) ?? [];
  const candidateIndex = forwarded.length - trustedProxyHops;
  return candidateIndex >= 0 ? forwarded[candidateIndex] : directPeer || undefined;
}
