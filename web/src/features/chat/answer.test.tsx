import { act, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { Citation } from "@/api/chat";
import { renderWithRouter } from "@/test/router";
import { Answer, AnswerText, announcement, copyText } from "./answer";

const CITATION: Citation = {
  n: 1,
  chunk_id: 7,
  file_id: "f0000000-0000-4000-8000-000000000001",
  file_name: "heron-notes.pdf",
  char_start: 120,
  char_end: 480,
  page: 3,
  quote: "The heron returned to the north pond at dawn.",
};

describe("AnswerText", () => {
  it("renders Markdown safely and [n] as citation chips", async () => {
    const { container } = await renderWithRouter(
      <AnswerText
        text={'**Herons** nest early [1]. <img src=x onerror="alert(1)"> [2] <script>x</script>'}
        citations={[CITATION]}
      />,
    );
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("script")).toBeNull();
    expect(container.textContent).toContain('<img src=x onerror="alert(1)">');
    expect(container.querySelector("strong")?.textContent).toBe("Herons");
    const chip = screen.getByRole("button", { name: "Source 1: heron-notes.pdf, page 3" });
    expect(chip).toHaveTextContent("1");
    // Unknown citation numbers stay text.
    expect(container.textContent).toContain("[2]");
    expect(screen.queryByRole("button", { name: /Source 2/ })).toBeNull();
  });

  it("shows the quote on tap and links to the page", async () => {
    const user = userEvent.setup();
    await renderWithRouter(<AnswerText text="Dawn [1]." citations={[CITATION]} />);
    await user.click(screen.getByRole("button", { name: /Source 1/ }));
    expect(await screen.findByText(CITATION.quote)).toBeInTheDocument();
    const link = screen.getByRole("link", { name: /Open at page 3/ });
    expect(link.getAttribute("href")).toBe(`/files/${CITATION.file_id}?at=120-480&page=3`);
  });
});

describe("Answer", () => {
  it("explains a refusal and offers a search", async () => {
    await renderWithRouter(
      <Answer
        question="capital of Mongolia"
        view={{
          text: "I couldn't find this in your files.",
          state: "refused",
          citable: [],
          cited: [],
        }}
      />,
    );
    expect(screen.getByText("I couldn't find this in your files.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /search for related passages/ })).toHaveAttribute(
      "href",
      "/search?q=capital+of+Mongolia",
    );
  });

  it("lists the passages when no model is configured", async () => {
    await renderWithRouter(
      <Answer
        question="heron"
        view={{ text: "", state: "no_llm", citable: [CITATION], cited: [CITATION] }}
      />,
    );
    expect(screen.getByText(/No language model is configured/)).toBeInTheDocument();
    expect(screen.getByRole("list", { name: "Sources" })).toHaveTextContent("heron-notes.pdf");
  });

  it("offers a retry after an error", async () => {
    let retried = 0;
    const user = userEvent.setup();
    await renderWithRouter(
      <Answer
        question="q"
        onRetry={() => {
          retried += 1;
        }}
        view={{ text: "Part", state: "error", citable: [], cited: [], error: "Model down." }}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("Model down.");
    await act(() => user.click(screen.getByRole("button", { name: "Try again" })));
    expect(retried).toBe(1);
  });
});

describe("copyText", () => {
  it("appends the cited sources", () => {
    expect(
      copyText({ text: "Dawn [1].", state: "answered", citable: [CITATION], cited: [CITATION] }),
    ).toBe("Dawn [1].\n\nSources:\n[1] heron-notes.pdf, p. 3");
  });
});

describe("answer announcements", () => {
  const base = { text: "", citable: [], cited: [] };
  it("speaks progress and the finished answer, never stored ones", () => {
    expect(announcement({ ...base, state: "sending" }, true)).toBe("Searching your files.");
    expect(announcement({ ...base, state: "streaming", text: "Par" }, true)).toBe(
      "Writing the answer.",
    );
    expect(announcement({ ...base, state: "answered", text: "It renews in May [1]." }, true)).toBe(
      "Answer ready. It renews in May.",
    );
    expect(announcement({ ...base, state: "answered", text: "Old." }, false)).toBe("");
    expect(announcement({ ...base, state: "refused" }, true)).toMatch(/No answer/);
  });
});
