import { describe, expect, it } from "vitest";
import { repairLatin1Utf8Mojibake } from "./utf8Mojibake";

/** Same corruption as `bytes[i] as char` in the host ANSI strip. */
function asByteChars(text: string): string {
  const bytes = new TextEncoder().encode(text);
  return Array.from(bytes, (b) => String.fromCharCode(b)).join("");
}

describe("repairLatin1Utf8Mojibake", () => {
  it("restores provider Chinese that was stored byte by byte", () => {
    const reason = "已超过大模型的免费额度 2026-11-19 18:30:01 适用";
    const garbled = asByteChars(reason);
    expect(garbled).not.toBe(reason);
    expect(garbled).toMatch(/å/);
    expect(repairLatin1Utf8Mojibake(garbled)).toBe(reason);
  });

  it("restores a mixed English prefix and garbled reason", () => {
    const plain =
      "Provider request failed after 0 attempts (budget 15): API error (status 429 Too Many Requests): 已超过大模型的免费额度";
    expect(repairLatin1Utf8Mojibake(asByteChars(plain))).toBe(plain);
  });

  it("restores the same sentence when Windows-1252 mapped the bytes", () => {
    const reason = "已超过大模型的免费额度 2026-11-19 18:30:01 适用";
    const garbled = new TextDecoder("windows-1252").decode(
      new TextEncoder().encode(reason),
    );
    expect(garbled).not.toBe(reason);
    expect(repairLatin1Utf8Mojibake(garbled)).toBe(reason);
  });

  it("leaves readable Chinese, English, and latin accents alone", () => {
    expect(repairLatin1Utf8Mojibake("请求被限速")).toBe("请求被限速");
    expect(
      repairLatin1Utf8Mojibake(
        "Provider request failed after 0 attempts (budget 15)",
      ),
    ).toBe("Provider request failed after 0 attempts (budget 15)");
    expect(repairLatin1Utf8Mojibake("café")).toBe("café");
  });
});
