import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { TabsContent, TabsList, TabsTrigger } from "./tabs";

describe("Tabs", () => {
  it("renders tablist, tab and tabpanel semantics", () => {
    const html = renderToString(
      <>
        <TabsList aria-label="Sections">
          <TabsTrigger id="tab-a" aria-controls="panel-a" active>
            Alpha
          </TabsTrigger>
          <TabsTrigger id="tab-b" aria-controls="panel-b">
            Beta
          </TabsTrigger>
        </TabsList>
        <TabsContent id="panel-a" aria-labelledby="tab-a">
          Panel A
        </TabsContent>
      </>,
    );
    expect(html).toContain('role="tablist"');
    expect(html).toContain('role="tab"');
    expect(html).toContain('aria-selected="true"');
    expect(html).toContain('aria-selected="false"');
    expect(html).toContain('aria-controls="panel-a"');
    expect(html).toContain('role="tabpanel"');
    expect(html).toContain('aria-labelledby="tab-a"');
  });

  it("marks triggers as type=button so they do not submit forms", () => {
    const html = renderToString(<TabsTrigger active>Alpha</TabsTrigger>);
    expect(html).toContain('type="button"');
  });
});
