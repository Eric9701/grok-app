import { describe, expect, it } from "vitest";
import { displayLibraryPath, isAbsoluteLibraryPath, wikiPathKey } from "./libraries";

describe("displayLibraryPath", () => {
  it("drops the Windows verbatim prefix", () => {
    expect(displayLibraryPath("\\\\?\\E:\\knowledge1\\test")).toBe("E:\\knowledge1\\test");
    expect(displayLibraryPath("\\\\?\\UNC\\server\\share")).toBe("\\\\server\\share");
    expect(displayLibraryPath("E:\\knowledge1\\test")).toBe("E:\\knowledge1\\test");
    expect(wikiPathKey("\\\\?\\E:\\knowledge1\\test")).toBe(
      wikiPathKey("E:\\knowledge1\\test"),
    );
    expect(isAbsoluteLibraryPath("\\\\?\\E:\\knowledge1\\test")).toBe(true);
  });
});
