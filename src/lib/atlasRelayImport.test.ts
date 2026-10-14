import { describe, expect, it } from "vitest";
import { parseAtlasRelayImport, splitRelayAddress } from "./atlasRelayImport";

const SAMPLE =
  "atlas agent --always-approve --model \"<你想要使用的模型名>\" headless --grok-ws-url 'ws://10.218.221.203:2420/ws?agent_id=9c7fc43585b0422e9cbfe93407f9b150-3&token=LUgbym_A0xKd-2c-t7ZUDV5goaC1WL8xaSqa93vO-OY' --grok-ws-origin 'http://10.218.221.203:2420'";

describe("parseAtlasRelayImport", () => {
  it("reads url, agent id, and token from an atlas agent command", () => {
    expect(parseAtlasRelayImport(SAMPLE)).toEqual({
      url: "ws://10.218.221.203:2420/ws",
      agentId: "9c7fc43585b0422e9cbfe93407f9b150-3",
      token: "LUgbym_A0xKd-2c-t7ZUDV5goaC1WL8xaSqa93vO-OY",
    });
  });

  it("accepts a double-quoted --grok-ws-url and a bare socket", () => {
    expect(
      parseAtlasRelayImport(
        '--grok-ws-url "wss://relay.example:2420/ws?agent_id=box-a&token=tok"',
      ),
    ).toEqual({
      url: "wss://relay.example:2420/ws",
      agentId: "box-a",
      token: "tok",
    });
    expect(
      parseAtlasRelayImport(
        "ws://10.0.0.8:2420/ws?agent_id=laptop-a&token=secret",
      ),
    ).toMatchObject({
      agentId: "laptop-a",
      token: "secret",
    });
  });

  it("ignores the origin and rejects text with no agent id", () => {
    expect(
      parseAtlasRelayImport("--grok-ws-origin 'http://10.218.221.203:2420'"),
    ).toBeNull();
    expect(parseAtlasRelayImport("")).toBeNull();
    expect(parseAtlasRelayImport("atlas agent --always-approve")).toBeNull();
  });
});

describe("splitRelayAddress", () => {
  it("leaves a plain host untouched", () => {
    expect(splitRelayAddress("ws://10.0.0.8:2420")).toBeNull();
  });
});
