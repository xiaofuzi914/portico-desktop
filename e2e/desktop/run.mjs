import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { readdirSync } from "node:fs";
import { createServer } from "node:net";
import { resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { remote } from "webdriverio";

const appDataDir = process.env.PORTICO_E2E_APP_DATA_DIR;
const providerName = process.env.PORTICO_E2E_PROVIDER_NAME;
if (!appDataDir || !providerName) {
  throw new Error(
    "PORTICO_E2E_APP_DATA_DIR and PORTICO_E2E_PROVIDER_NAME are required",
  );
}

const binary = resolve(
  process.env.PORTICO_E2E_BINARY ?? "target/desktop-e2e/release/portico-tauri",
);
const expectedMigrationCount = readdirSync(
  resolve("crates/app-runtime/migrations"),
).filter((name) => /^\d+_.+\.sql$/.test(name)).length;
let activeChild;

function hasExited(child) {
  return child.exitCode !== null || child.signalCode !== null;
}

function waitForExit(child, timeoutMs) {
  if (hasExited(child)) return Promise.resolve(true);
  return new Promise((resolveExit) => {
    const onExit = () => {
      clearTimeout(timer);
      resolveExit(true);
    };
    const timer = setTimeout(() => {
      child.removeListener("exit", onExit);
      resolveExit(false);
    }, timeoutMs);
    child.once("exit", onExit);
  });
}

async function stopProcess(child) {
  if (hasExited(child)) return;
  child.kill("SIGTERM");
  if (await waitForExit(child, 5_000)) return;
  child.kill("SIGKILL");
  if (!(await waitForExit(child, 5_000))) {
    throw new Error(`failed to stop Portico E2E process ${child.pid}`);
  }
}

for (const [signal, exitCode] of [
  ["SIGINT", 130],
  ["SIGTERM", 143],
]) {
  process.once(signal, () => {
    if (activeChild && !hasExited(activeChild)) activeChild.kill("SIGKILL");
    process.exit(exitCode);
  });
}

function allocatePort() {
  return new Promise((resolvePort, reject) => {
    const server = createServer();
    server.unref();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      assert(address && typeof address === "object");
      server.close((error) =>
        error ? reject(error) : resolvePort(address.port),
      );
    });
  });
}

async function waitForServer(port, child) {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    if (hasExited(child)) {
      throw new Error(
        `Portico exited before WebDriver was ready (code=${child.exitCode}, signal=${child.signalCode})`,
      );
    }
    try {
      const response = await fetch(`http://127.0.0.1:${port}/status`, {
        signal: AbortSignal.timeout(1_000),
      });
      if (response.ok) return;
    } catch {
      // The native app and embedded server are still starting.
    }
    await delay(100);
  }
  throw new Error(`embedded WebDriver did not become ready on port ${port}`);
}

async function openSettings(client) {
  const settingsLink = await client.$("a[href='/settings']");
  await settingsLink.waitForClickable({ timeout: 30_000 });
  await settingsLink.click();
  await (
    await client.$("[data-testid='settings-page']")
  ).waitForDisplayed({ timeout: 30_000 });
}

async function assertMigrations(client) {
  await (await client.$("[data-testid='list-migrations']")).click();
  await client.waitUntil(
    async () =>
      (await client.$$("[data-testid='migrations-table'] tbody tr")).length ===
      expectedMigrationCount,
    {
      timeout: 30_000,
      timeoutMsg: `expected all ${expectedMigrationCount} durable migrations through real Tauri IPC`,
    },
  );
}

async function assertHighRiskCommandsAreUnavailable(client) {
  const commands = [
    "git_stage",
    "create_terminal",
    "invoke_mcp_tool",
    "open_browser_window",
    "capture_screen",
    "create_automation",
    "execute_subagents",
  ];
  const results = await client.execute(async (names) => {
    return Promise.all(
      names.map(async (name) => {
        try {
          await window.__TAURI_INTERNALS__.invoke(name, {});
          return { name, unavailable: false };
        } catch (error) {
          return { name, unavailable: true, error: String(error) };
        }
      }),
    );
  }, commands);
  for (const result of results) {
    assert.equal(
      result.unavailable,
      true,
      `${result.name} unexpectedly crossed production IPC`,
    );
    assert.match(
      result.error ?? "",
      /command .+ not found/i,
      `${result.name} failed for a reason other than being unregistered: ${result.error}`,
    );
  }
}

async function runNativePhase(port, phase) {
  const child = spawn(binary, [], {
    env: {
      HOME: appDataDir,
      LANG: process.env.LANG ?? "en_US.UTF-8",
      PATH: process.env.PATH ?? "/usr/bin:/bin",
      PORTICO_E2E_APP_DATA_DIR: appDataDir,
      TAURI_WEBDRIVER_PORT: String(port),
      TMPDIR: appDataDir,
      WDIO_EMBEDDED_SERVER: "true",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  activeChild = child;
  child.stdout.pipe(process.stdout);
  child.stderr.pipe(process.stderr);

  let client;
  try {
    await waitForServer(port, child);
    client = await remote({
      hostname: "127.0.0.1",
      port,
      path: "/",
      logLevel: "warn",
      connectionRetryTimeout: 30_000,
      capabilities: { browserName: "tauri" },
    });

    await client.waitUntil(async () => (await client.$$("body *")).length > 0, {
      timeout: 30_000,
      timeoutMsg: "Portico webview did not render",
    });
    assert.equal(await client.getTitle(), "Portico");
    assert.deepEqual(await client.getWindowHandles(), ["main"]);
    await assertHighRiskCommandsAreUnavailable(client);
    await openSettings(client);
    await assertMigrations(client);

    if (phase === "seed") {
      await (await client.$("[data-testid='collect-diagnostics']")).click();
      const diagnostics = await client.$("[data-testid='diagnostics-result']");
      await diagnostics.waitForDisplayed({ timeout: 30_000 });
      const diagnosticsText = await diagnostics.getText();
      assert.match(diagnosticsText, /diagnostics/i);
      assert.match(diagnosticsText, /Redacted:\s+Yes/i);

      await (await client.$("a[href='/models']")).click();
      await (
        await client.$("[data-testid='provider-name']")
      ).setValue(providerName);
      await (
        await client.$("[data-testid='provider-base-url']")
      ).setValue("http://127.0.0.1:9/v1");
      assert.equal(
        await (
          await client.$("[data-testid='provider-key-reference']")
        ).getValue(),
        "openai-default",
      );
      await (await client.$("[data-testid='add-provider']")).click();
    } else {
      await (await client.$("a[href='/models']")).click();
    }

    const providerList = await client.$("[data-testid='provider-list']");
    await providerList.waitForDisplayed({ timeout: 30_000 });
    await client.waitUntil(
      async () => (await providerList.getText()).includes(providerName),
      {
        timeout: 30_000,
        timeoutMsg: `provider was not visible after ${phase}`,
      },
    );
    console.log(`desktop-e2e-${phase}-ok`);
  } finally {
    if (client) await client.deleteSession().catch(() => undefined);
    await stopProcess(child);
    activeChild = undefined;
  }
}

await runNativePhase(await allocatePort(), "seed");
await runNativePhase(await allocatePort(), "verify");
