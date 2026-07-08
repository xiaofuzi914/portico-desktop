import { describe, expect, it } from "vitest";
import {
  defaultLanguage,
  getLanguageLabel,
  isSupportedLanguage,
  normalizeLanguage,
  translate,
} from "./i18n";

describe("i18n", () => {
  it("uses English as the default language", () => {
    expect(defaultLanguage).toBe("en");
    expect(translate("en", "nav.projects")).toBe("Projects");
  });

  it("translates product navigation into Chinese", () => {
    expect(translate("zh", "nav.projects")).toBe("项目");
    expect(translate("zh", "agent.startRun")).toBe("开始运行");
  });

  it("presents thread-backed work as sessions in user-facing copy", () => {
    expect(translate("en", "nav.threads")).toBe("Sessions");
    expect(translate("en", "sidebar.newThread")).toBe("New session");
    expect(translate("en", "thread.threadTitle")).toBe("Session title");
    expect(translate("en", "thread.defaultTitle")).toBe("New session");
    expect(translate("zh", "nav.threads")).toBe("会话");
    expect(translate("zh", "sidebar.newThread")).toBe("新建会话");
    expect(translate("zh", "thread.threadTitle")).toBe("会话标题");
    expect(translate("zh", "thread.defaultTitle")).toBe("新会话");
    expect(translate("zh", "memory.scope.thread")).toBe("当前会话");
  });

  it("falls back to English for unknown keys", () => {
    expect(translate("zh", "missing.key")).toBe("missing.key");
  });

  it("normalizes unsupported language values", () => {
    expect(isSupportedLanguage("zh")).toBe(true);
    expect(isSupportedLanguage("fr")).toBe(false);
    expect(normalizeLanguage("fr")).toBe(defaultLanguage);
  });

  it("returns readable language labels", () => {
    expect(getLanguageLabel("en")).toBe("English");
    expect(getLanguageLabel("zh")).toBe("中文");
  });
});
