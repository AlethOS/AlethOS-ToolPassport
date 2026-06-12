import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("dashboard motion accessibility", () => {
  it("disables non-essential animation for reduced-motion users", () => {
    const stylesPath = resolve(process.cwd(), "app/styles.css");
    const styles = readFileSync(stylesPath, "utf8");

    expect(styles).toContain("@media (prefers-reduced-motion: reduce)");
    expect(styles).toContain("animation-duration: 0.001ms");
  });
});
