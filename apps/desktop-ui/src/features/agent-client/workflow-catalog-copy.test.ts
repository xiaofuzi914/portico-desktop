import { describe, expect, it } from "vitest";
import {
  catalogDisplayOrder,
  isKnownCatalogKey,
  resolveCatalogCopy,
} from "./workflow-catalog-copy";

const t = (key: string) => {
  const map: Record<string, string> = {
    "orchestration.wf.multi-lens-review.title": "多角度检查",
    "orchestration.wf.multi-lens-review.when": "适合：要挑错、评审、对照清单",
    "orchestration.wf.multi-lens-review.diff": "会从几个角度分别看，再汇总成一份结论",
    "orchestration.wf.deep-research.title": "深入摸清",
    "orchestration.wf.deep-research.when": "适合：要搞清结构、来龙去脉、证据",
    "orchestration.wf.deep-research.diff": "先拆问题，分头查，再写成研究报告",
    "orchestration.wf.iterative-refine.title": "反复打磨",
    "orchestration.wf.iterative-refine.when": "适合：草稿要改到能交差",
    "orchestration.wf.iterative-refine.diff": "写一版 → 挑问题 → 再改，有次数上限",
    "orchestration.wf.custom.when": "你保存过的协作方式",
    "orchestration.wf.custom.diff": "按你保存的步骤执行",
  };
  return map[key] ?? key;
};

describe("workflow-catalog-copy", () => {
  it("maps known keys to product language", () => {
    const copy = resolveCatalogCopy("multi-lens-review", t, "Multi-lens review", "fan-out");
    expect(copy.title).toBe("多角度检查");
    expect(copy.when).toContain("适合");
    expect(copy.difference).not.toMatch(/fan-out|foreach|DAG/i);
  });

  it("falls back for custom templates without jargon requirement", () => {
    const copy = resolveCatalogCopy("uuid-custom", t, "我的流程", "自己写的");
    expect(copy.title).toBe("我的流程");
    expect(copy.when).toBe("自己写的");
  });

  it("orders built-ins before unknowns", () => {
    const ids = ["iterative-refine", "custom-x", "multi-lens-review", "deep-research"];
    ids.sort(catalogDisplayOrder);
    expect(ids.slice(0, 3)).toEqual([
      "multi-lens-review",
      "deep-research",
      "iterative-refine",
    ]);
  });

  it("detects known catalog keys", () => {
    expect(isKnownCatalogKey("deep-research")).toBe(true);
    expect(isKnownCatalogKey("nope")).toBe(false);
  });
});
