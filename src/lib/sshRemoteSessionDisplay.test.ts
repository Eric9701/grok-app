import { describe, expect, it } from "vitest";
import {
  cwdBasename,
  firstSentenceTitle,
  looksLikeSessionId,
  remainingRemoteCount,
  remotePathTip,
  remoteSessionLabel,
  remoteTitleKey,
} from "./sshRemoteSessionDisplay";

describe("remoteSessionLabel", () => {
  const untitled = "未命名";

  it("prefers a custom name", () => {
    expect(
      remoteSessionLabel({
        title: "01a01907-adf3-7e00-a7a8-aee1082b0556",
        cwd: "/data/pengqlu/code/idea",
        custom: "ICLR draft",
        untitled,
      }),
    ).toBe("ICLR draft");
  });

  it("uses the first sentence instead of a UUID", () => {
    expect(
      remoteSessionLabel({
        title: "帮我看一下 hallucination span 标注",
        cwd: "/data/pengqlu/code/qwen35-v001-light",
        untitled,
      }),
    ).toBe("帮我看一下 hallucination span 标注");
  });

  it("falls back to the cwd basename when title is the session id", () => {
    expect(
      remoteSessionLabel({
        title: "01a01907-adf3-7e00-a7a8-aee1082b0556",
        cwd: "/data/pengqlu/code/qwen35-v001-light",
        untitled,
      }),
    ).toBe("qwen35-v001-light");
  });

  it("uses untitled when nothing readable exists", () => {
    expect(
      remoteSessionLabel({
        title: "01a01907-adf3-7e00-a7a8-aee1082b0556",
        cwd: "",
        untitled,
      }),
    ).toBe(untitled);
  });
});

describe("helpers", () => {
  it("detects grok session ids", () => {
    expect(looksLikeSessionId("01a01907-adf3-7e00-a7a8-aee1082b0556")).toBe(
      true,
    );
    expect(looksLikeSessionId("帮我看一下")).toBe(false);
  });

  it("collapses the first sentence", () => {
    expect(firstSentenceTitle("  hello\nworld  ")).toBe("hello");
  });

  it("builds hover copy with alias and full path", () => {
    expect(remotePathTip("UTS", "/data/pengqlu/code/idea")).toBe(
      "UTS · /data/pengqlu/code/idea",
    );
  });

  it("counts remaining rows for load more", () => {
    expect(remainingRemoteCount(35, 20)).toBe(15);
    expect(remainingRemoteCount(10, 20)).toBe(0);
  });

  it("keys overlay titles by host and id", () => {
    expect(remoteTitleKey("UTS", "abc")).toBe("UTS:abc");
  });

  it("takes the last path segment", () => {
    expect(cwdBasename("/data/pengqlu/code/qwen35-v001-light")).toBe(
      "qwen35-v001-light",
    );
  });
});
