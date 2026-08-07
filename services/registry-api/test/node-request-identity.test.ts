import { describe, expect, it } from "vitest";

import { trustedClientIp } from "../src/node-request-identity";

describe("Node trusted-proxy identity", () => {
  it("ignores forwarded identity without a configured trusted proxy", () => {
    expect(trustedClientIp("198.51.100.9", "203.0.113.7", 0)).toBe("203.0.113.7");
  });

  it("selects identity from the trusted right edge of the forwarding chain", () => {
    expect(trustedClientIp("192.0.2.99, 198.51.100.9", "203.0.113.7", 1)).toBe("198.51.100.9");
    expect(trustedClientIp("192.0.2.99, 198.51.100.9", "203.0.113.7", 2)).toBe("192.0.2.99");
  });

  it("falls back to the direct peer when the configured chain is absent", () => {
    expect(trustedClientIp(undefined, "203.0.113.7", 1)).toBe("203.0.113.7");
  });
});
