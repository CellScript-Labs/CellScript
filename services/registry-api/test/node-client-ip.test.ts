import { describe, expect, it } from "vitest";

import { applyNodeClientIdentity, parseTrustedProxyHops, resolveNodeClientIp } from "../src/node-client-ip";

describe("Node client IP boundary", () => {
  it("ignores forwarding headers unless proxy trust is explicit", () => {
    expect(resolveNodeClientIp("::ffff:127.0.0.1", "198.51.100.7", 0)).toBe("127.0.0.1");
  });

  it("selects from the trusted end of an appended forwarding chain", () => {
    expect(resolveNodeClientIp("10.0.0.9", "192.0.2.99, 203.0.113.7", 1)).toBe("203.0.113.7");
    expect(resolveNodeClientIp("10.0.0.9", "192.0.2.99, 203.0.113.7, 10.0.0.8", 2)).toBe("203.0.113.7");
  });

  it("falls back to the peer when the trusted suffix is missing or malformed", () => {
    expect(resolveNodeClientIp("10.0.0.9", "203.0.113.7", 2)).toBe("10.0.0.9");
    expect(resolveNodeClientIp("10.0.0.9", "203.0.113.7, not-an-ip", 1)).toBe("10.0.0.9");
  });

  it("removes caller-controlled identity headers before setting the canonical address", () => {
    const headers = new Headers({
      "cf-asn": "64512",
      "cf-connecting-ip": "192.0.2.99",
      "x-forwarded-for": "192.0.2.99, 203.0.113.7",
    });
    applyNodeClientIdentity(headers, "10.0.0.9", headers.get("x-forwarded-for") ?? undefined, 1);
    expect(headers.get("cf-connecting-ip")).toBe("203.0.113.7");
    expect(headers.has("cf-asn")).toBe(false);
    expect(headers.has("x-forwarded-for")).toBe(false);
  });

  it("validates the configured trust depth", () => {
    expect(parseTrustedProxyHops(undefined)).toBe(0);
    expect(parseTrustedProxyHops(" 2 ")).toBe(2);
    for (const invalid of ["-1", "1.5", "17", "proxy"]) {
      expect(() => parseTrustedProxyHops(invalid)).toThrow("0..16");
    }
  });
});
