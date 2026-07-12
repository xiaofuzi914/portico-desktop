import {
  Document,
  HeadingLevel,
  Packer,
  Paragraph,
  Table,
  TableCell,
  TableRow,
  TextRun,
  type FileChild,
  type ParagraphChild,
} from "docx";

type HeadingLevelValue = (typeof HeadingLevel)[keyof typeof HeadingLevel];

function textRuns(element: Element): ParagraphChild[] {
  const runs: ParagraphChild[] = [];
  element.childNodes.forEach((node) => {
    if (node.nodeType === Node.TEXT_NODE) {
      if (node.textContent) runs.push(new TextRun(node.textContent));
      return;
    }
    if (!(node instanceof Element)) return;
    const text = node.textContent ?? "";
    if (node.tagName === "BR") runs.push(new TextRun({ break: 1 }));
    else if (text) {
      runs.push(
        new TextRun({
          text,
          bold: node.matches("strong,b"),
          italics: node.matches("em,i"),
          strike: node.matches("del,s"),
          font: node.matches("code") ? "Consolas" : undefined,
        }),
      );
    }
  });
  return runs.length > 0 ? runs : [new TextRun(element.textContent ?? "")];
}

function paragraphFrom(
  element: Element,
  options: { heading?: HeadingLevelValue; bullet?: number; quote?: boolean } = {},
) {
  return new Paragraph({
    children: textRuns(element),
    heading: options.heading,
    bullet: options.bullet === undefined ? undefined : { level: options.bullet },
    indent: options.quote ? { left: 360 } : undefined,
  });
}

function tableFrom(element: Element): Table | null {
  const rows = Array.from(element.querySelectorAll("tr"));
  if (rows.length === 0) return null;
  return new Table({
    rows: rows.map(
      (row) =>
        new TableRow({
          children: Array.from(row.children).map(
            (cell) => new TableCell({ children: [paragraphFrom(cell)] }),
          ),
        }),
    ),
  });
}

export function renderedMarkdownToDocxChildren(root: Element): FileChild[] {
  const children: FileChild[] = [];
  const headings: Record<string, HeadingLevelValue> = {
    h1: HeadingLevel.HEADING_1,
    h2: HeadingLevel.HEADING_2,
    h3: HeadingLevel.HEADING_3,
    h4: HeadingLevel.HEADING_4,
    h5: HeadingLevel.HEADING_5,
    h6: HeadingLevel.HEADING_6,
  };
  Array.from(root.children).forEach((element) => {
    const tag = element.tagName.toLowerCase();
    const heading = headings[tag];
    if (heading) children.push(paragraphFrom(element, { heading }));
    else if (tag === "ul" || tag === "ol") {
      Array.from(element.children)
        .filter((item) => item.tagName.toLowerCase() === "li")
        .forEach((item) => children.push(paragraphFrom(item, tag === "ul" ? { bullet: 0 } : {})));
    } else if (tag === "blockquote") children.push(paragraphFrom(element, { quote: true }));
    else if (tag === "pre") {
      children.push(
        new Paragraph({
          children: [new TextRun({ text: element.textContent ?? "", font: "Consolas" })],
        }),
      );
    } else if (tag === "table") {
      const table = tableFrom(element);
      if (table) children.push(table);
    } else if (tag !== "hr") children.push(paragraphFrom(element));
  });
  return children;
}

export async function buildMarkdownDocx(root: Element): Promise<Blob> {
  const document = new Document({
    sections: [{ children: renderedMarkdownToDocxChildren(root) }],
  });
  return Packer.toBlob(document);
}
