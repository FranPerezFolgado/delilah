// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, type Conversation, type ConversationEntry, type Session } from "../api";
import { setLanguage } from "../i18n";
import { ConversationView } from "./ConversationView";

const live = vi.hoisted(() => ({ handlers: new Map<string, Set<() => void>>() }));
vi.mock("../live", () => ({
  on: (topic: string, cb: () => void) => {
    if (!live.handlers.has(topic)) live.handlers.set(topic, new Set());
    live.handlers.get(topic)!.add(cb);
    return () => live.handlers.get(topic)!.delete(cb);
  },
}));
const emit = (topic: string) => act(() => live.handlers.get(topic)?.forEach((cb) => cb()));

beforeEach(() => {
  setLanguage("es");
  live.handlers.clear();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const session = (over: Partial<Session> = {}): Session =>
  ({ id: "s1", task: "p", kind: "claude", label: "idea", primary: true, closed: false, live: true, read_only: false, state: "waiting", conversation: true, ...over }) as Session;

const conv = (entries: ConversationEntry[] = [], over: Partial<Conversation> = {}): Conversation => ({
  entries,
  cursor: "0",
  complete: true,
  transcript: "present",
  activity: null,
  needs_terminal: false,
  runtime: "stream",
  ...over,
});

const user = (id: number, text: string, state: "queued" | "sent" | "delivered" = "delivered"): ConversationEntry => ({ id: String(id), at: "", kind: "user", text, state, from: "view" });
const claude = (id: number, text: string): ConversationEntry => ({ id: String(id), at: "", kind: "claude", text });
const card = (id: number, over: object = {}): ConversationEntry => ({
  id: String(id),
  at: "",
  kind: "card",
  ask: "ask-1",
  questions: [{ question: "¿Cuál?", options: [{ label: "A" }, { label: "B" }] }],
  proposal: null,
  outcome: "pending",
  answer: null,
  task: null,
  error: null,
  ...over,
});

const tool = (id: string, at: string, over: Partial<Extract<ConversationEntry, { kind: "tool" }>> = {}): ConversationEntry => ({
  id,
  at,
  kind: "tool",
  tool: "Read",
  tool_use_id: id.replace("t:", ""),
  summary: "Leyó main.rs",
  input: {},
  result: null,
  is_error: false,
  truncated: false,
  ...over,
});
const at = (id: string, at: string, text: string, kind: "user" | "claude" = "claude"): ConversationEntry =>
  kind === "claude" ? { id, at, kind, text } : { id, at, kind, text, state: "delivered", from: "terminal" };

function setup(c: Conversation, s = session(), cbs: Partial<Parameters<typeof ConversationView>[0]> = {}) {
  vi.spyOn(api, "conversation").mockResolvedValue(c);
  return render(<ConversationView session={s} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} {...cbs} />);
}

describe("ConversationView", () => {
  it("shows the hint with no entries", async () => {
    const { findByText } = setup(conv());
    expect(await findByText("Cuéntale tu idea…")).toBeTruthy();
  });

  it("renders entries in order and a pending card as an AskCard", async () => {
    const { container, findByText } = setup(conv([user(1, "hola"), claude(2, "**listo**"), card(3)]));
    await findByText("¿Cuál?");
    const kids = [...container.querySelector(".conv-log")!.children];
    expect(kids.map((k) => k.className)).toEqual(["conv-msg conv-user selectable", "conv-msg conv-claude selectable", "conv-card"]);
    expect(kids[1].querySelector("strong")?.textContent).toBe("listo");
    expect(kids[2].querySelector(".ask")).toBeTruthy();
  });

  it("removes a queued message", async () => {
    const rm = vi.spyOn(api, "removeMessage").mockResolvedValue(null);
    const { findByText, getByLabelText } = setup(conv([user(4, "luego", "queued")]));
    await findByText("en cola");
    fireEvent.click(getByLabelText("Quitar mensaje"));
    expect(rm).toHaveBeenCalledWith("s1", "4");
  });

  it("sends with Enviar and with cmd+enter, clears, and cannot send empty", async () => {
    const send = vi.spyOn(api, "sendMessage").mockResolvedValue(user(9, "x"));
    const { findByText, getByLabelText, getByText } = setup(conv(), session({ state: "working" }));
    await findByText("Cuéntale tu idea…");
    const field = getByLabelText("Mensaje") as HTMLTextAreaElement;
    expect((getByText("Enviar") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(field, { target: { value: "idea uno" } });
    fireEvent.click(getByText("Enviar"));
    await waitFor(() => expect(send).toHaveBeenCalledWith("s1", "idea uno"));
    expect(field.value).toBe("");
    fireEvent.change(field, { target: { value: "idea dos" } });
    fireEvent.keyDown(field, { key: "Enter", metaKey: true });
    await waitFor(() => expect(send).toHaveBeenCalledWith("s1", "idea dos"));
  });

  it("shows Detener only while working, and stops", async () => {
    const stop = vi.spyOn(api, "stopSession").mockResolvedValue(null);
    const a = setup(conv(), session({ state: "working" }));
    fireEvent.click(await a.findByText("Detener"));
    expect(stop).toHaveBeenCalledWith("s1");
    cleanup();
    const b = setup(conv(), session({ state: "waiting" }));
    await b.findByText("Cuéntale tu idea…");
    expect(b.queryByText("Detener")).toBeNull();
  });

  it("shows the activity while working", async () => {
    const { findByText } = setup(conv([], { activity: "Leyendo main.rs" }), session({ state: "working" }));
    expect(await findByText("Leyendo main.rs")).toBeTruthy();
  });

  it("offers the terminal when Claude waits there", async () => {
    const onOpenTerminal = vi.fn();
    const { findByText, getByText } = setup(conv([], { needs_terminal: true }), session(), { onOpenTerminal });
    await findByText("Claude espera en la terminal");
    fireEvent.click(getByText("Abrir terminal"));
    expect(onOpenTerminal).toHaveBeenCalled();
  });

  it("offers Reabrir when the session is not live", async () => {
    const onReopen = vi.fn();
    const { findByText } = setup(conv(), session({ live: false }), { onReopen });
    fireEvent.click(await findByText("Reabrir"));
    expect(onReopen).toHaveBeenCalled();
  });

  it("renders a permission card and answers it through the session", async () => {
    const perm: ConversationEntry = { id: "perm:p1", at: "", kind: "card", ask_kind: "permission", ask: "p1", tool: "Bash", summary: "npm test", suggestion: null, rule: "Bash(npm test)", outcome: "pending" };
    const answer = vi.spyOn(api, "answerPermission").mockResolvedValue(null);
    const { findByText } = setup(conv([perm]));
    expect(await findByText("npm test")).toBeTruthy();
    expect(await findByText("Bash(npm test)")).toBeTruthy();
    fireEvent.click(await findByText("Permitir una vez"));
    await waitFor(() => expect(answer).toHaveBeenCalledWith("s1", "p1", "allow_once", undefined));
  });

  it("exposes the log as Conversación and a skeleton while loading", async () => {
    vi.spyOn(api, "conversation").mockReturnValue(new Promise(() => {}));
    const { container } = render(<ConversationView session={session()} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />);
    expect(container.querySelector('[role="log"]')?.getAttribute("aria-label")).toBe("Conversación");
    expect(container.querySelector(".conv-log .sk")).toBeTruthy();
  });

  it("keeps an expired permission card with its outcome", async () => {
    const perm: ConversationEntry = { id: "perm:p2", at: "", kind: "card", ask_kind: "permission", ask: "p2", tool: "Bash", summary: "rm x", suggestion: null, rule: null, outcome: "terminal" };
    const { findByText, queryByText, container } = setup(conv([perm]));
    expect(await findByText("Caducada · se respondió en la terminal")).toBeTruthy();
    expect(queryByText("Permitir una vez")).toBeNull();
    expect(container.querySelector(".ac-chat")).toBeTruthy();
  });

  it("shows the decision of an answered permission card", async () => {
    const perm: ConversationEntry = { id: "perm:p3", at: "", kind: "card", ask_kind: "permission", ask: "p3", tool: "Bash", summary: "ls", suggestion: null, rule: null, outcome: "answered", decision: "deny" };
    const { findByText } = setup(conv([perm]));
    expect(await findByText("Denegada")).toBeTruthy();
  });

  it("words the stopped marker as Detenida: detuviste a Claude.", async () => {
    const { findByText } = setup(conv([{ id: "m1", at: "", kind: "marker", marker: "stopped" }]));
    expect(await findByText("Detenida: detuviste a Claude.")).toBeTruthy();
  });

  it("shows the ⌘↵ hint on Enviar", async () => {
    const { findByRole } = setup(conv());
    expect((await findByRole("button", { name: /Enviar/ })).textContent).toContain("⌘↵");
  });

  it("shows an answered card's task as a link", async () => {
    const onOpenTask = vi.fn();
    const { findByText } = setup(conv([card(5, { outcome: "answered", answer: { "¿Cuál?": "Crear" }, task: "LIBRE-7" })]), session(), { onOpenTask });
    const link = await findByText("LIBRE-7");
    expect(link.tagName).toBe("A");
    fireEvent.click(link);
    expect(onOpenTask).toHaveBeenCalledWith("LIBRE-7");
  });

  it("labels cards answered in the terminal and withdrawn", async () => {
    const { findByText } = setup(conv([card(6, { outcome: "terminal" }), card(7, { outcome: "withdrawn" })]));
    expect(await findByText("Respondida en la terminal")).toBeTruthy();
    expect(await findByText("Retirada")).toBeTruthy();
  });

  it("does not render an answered card as interactive", async () => {
    const { container, findByText } = setup(conv([card(8, { outcome: "answered", answer: { "¿Cuál?": "A" } })]));
    await findByText("¿Cuál?: A");
    expect(container.querySelector(".ask")).toBeNull();
  });

  it("ignores a stale response after switching session", async () => {
    let slow!: (c: Conversation) => void;
    const spy = vi.spyOn(api, "conversation").mockImplementation((sid: string) =>
      sid === "s1" ? new Promise<Conversation>((r) => (slow = r)) : Promise.resolve(conv([claude(1, "de dos")])),
    );
    const props = { onOpenTerminal: () => {}, onOpenTask: () => {}, onReopen: () => {} };
    const { rerender, findByText, queryByText } = render(<ConversationView session={session()} {...props} />);
    rerender(<ConversationView session={session({ id: "s2" })} {...props} />);
    await findByText("de dos");
    slow(conv([claude(2, "de uno")]));
    await new Promise((r) => setTimeout(r, 10));
    expect(queryByText("de uno")).toBeNull();
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it("clears the error after a successful reload", async () => {
    const spy = vi.spyOn(api, "conversation").mockRejectedValueOnce(new Error("caído")).mockResolvedValue(conv([claude(1, "ok")]));
    const { findByText, queryByText } = render(
      <ConversationView session={session()} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />,
    );
    await findByText("caído");
    // A successful send reloads the conversation.
    vi.spyOn(api, "sendMessage").mockResolvedValue(user(2, "x"));
    fireEvent.change(document.querySelector("textarea")!, { target: { value: "hola" } });
    fireEvent.click(document.querySelector(".b-p") as HTMLButtonElement);
    await waitFor(() => expect(queryByText("caído")).toBeNull());
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it("keeps text typed while a send fails", async () => {
    let fail!: (e: Error) => void;
    vi.spyOn(api, "sendMessage").mockImplementation(() => new Promise((_, rej) => (fail = rej)));
    const { findByText, getByLabelText, getByText } = setup(conv());
    await findByText("Cuéntale tu idea…");
    const field = getByLabelText("Mensaje") as HTMLTextAreaElement;
    fireEvent.change(field, { target: { value: "primero" } });
    fireEvent.click(getByText("Enviar"));
    await waitFor(() => expect(field.value).toBe(""));
    fireEvent.change(field, { target: { value: "segundo" } });
    fail(new Error("falló"));
    await findByText("falló");
    expect(field.value).toBe("segundo");
  });

  it("restores the message when a send fails and the box is still empty", async () => {
    vi.spyOn(api, "sendMessage").mockRejectedValue(new Error("falló"));
    const { findByText, getByLabelText, getByText } = setup(conv());
    await findByText("Cuéntale tu idea…");
    const field = getByLabelText("Mensaje") as HTMLTextAreaElement;
    fireEvent.change(field, { target: { value: "primero" } });
    fireEvent.click(getByText("Enviar"));
    await findByText("falló");
    expect(field.value).toBe("primero");
  });

  it("reopens a closed conversation with its full history in order", async () => {
    const onReopen = vi.fn();
    const { container, findByText } = setup(
      conv([user(1, "idea"), claude(2, "respuesta"), card(3, { outcome: "answered", answer: { "¿Cuál?": "A" } }), claude(4, "fin")]),
      session({ live: false, closed: true }),
      { onReopen },
    );
    await findByText("fin");
    const kids = [...container.querySelector(".conv-log")!.children].map((k) => k.textContent);
    expect(kids).toEqual(["idea", "respuesta", "¿Cuál?: A", "fin"]);
    fireEvent.click(await findByText("Reabrir"));
    expect(onReopen).toHaveBeenCalled();
  });

  it("renders user, claude, tool and marker entries of a task session in order", async () => {
    const entries: ConversationEntry[] = [
      at("t:1", "2026-01-01T00:00:01Z", "haz esto", "user"),
      at("t:2", "2026-01-01T00:00:02Z", "vale"),
      tool("t:3", "2026-01-01T00:00:03Z"),
      { id: "t:4", at: "2026-01-01T00:00:04Z", kind: "marker", marker: "compacted" },
    ];
    const { container, findByText } = setup(conv(entries), session({ conversation: false }));
    await findByText("Leyó main.rs");
    const kids = [...container.querySelector(".conv-log")!.children];
    expect(kids.map((k) => k.className.split(" ")[0])).toEqual(["conv-msg", "conv-msg", "tool-card", "conv-marker"]);
    expect(kids[0].textContent).toBe("haz esto");
    expect(kids[3].textContent).toBeTruthy();
  });

  it("fetches again with the cursor on a conversations event and merges entries", async () => {
    const first = conv([at("t:1", "2026-01-01T00:00:01Z", "uno"), tool("t:2", "2026-01-01T00:00:02Z"), user(5, "viejo")], { cursor: "c1" });
    const second = conv(
      [at("t:3", "2026-01-01T00:00:03Z", "tres"), tool("t:2", "2026-01-01T00:00:02Z", { result: "ok", summary: "Leyó main.rs (listo)" }), claude(6, "nuevo")],
      { cursor: "c2" },
    );
    const spy = vi.spyOn(api, "conversation").mockResolvedValueOnce(first).mockResolvedValue(second);
    const { container, findByText, queryByText } = render(
      <ConversationView session={session()} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />,
    );
    await findByText("viejo");
    emit("conversations");
    await findByText("tres");
    expect(spy).toHaveBeenLastCalledWith("s1", "c1");
    expect(queryByText("viejo")).toBeNull();
    expect(queryByText("Leyó main.rs")).toBeNull();
    expect(container.querySelectorAll(".tool-card")).toHaveLength(1);
    expect(container.textContent).toContain("uno");
    expect(container.textContent).toContain("nuevo");
  });

  it("drops the earlier ask tool entry when its card arrives in a later poll", async () => {
    const ask = tool("t:ask1", "2026-01-01T00:00:01Z", { tool: "AskUserQuestion", summary: "Preguntó" });
    const first = conv([ask], { cursor: "c1" });
    const second = conv([card(7, { tool_use_id: "ask1", at: "2026-01-01T00:00:01Z" })], { cursor: "c2" });
    vi.spyOn(api, "conversation").mockResolvedValueOnce(first).mockResolvedValue(second);
    const { container, findByText, queryByText } = render(
      <ConversationView session={session()} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />,
    );
    await findByText("Preguntó");
    emit("conversations");
    await findByText("¿Cuál?");
    expect(queryByText("Preguntó")).toBeNull();
    expect(container.querySelectorAll(".tool-card")).toHaveLength(0);
  });

  it("fetches again right away when the answer is not complete", async () => {
    const spy = vi
      .spyOn(api, "conversation")
      .mockResolvedValueOnce(conv([at("t:1", "2026-01-01T00:00:01Z", "uno")], { cursor: "c1", complete: false }))
      .mockResolvedValue(conv([at("t:2", "2026-01-01T00:00:02Z", "dos")], { cursor: "c2" }));
    const { findByText } = render(<ConversationView session={session()} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />);
    await findByText("dos");
    expect(spy).toHaveBeenCalledTimes(2);
    expect(spy).toHaveBeenLastCalledWith("s1", "c1");
    await findByText("uno");
  });

  it("folds a notification and its hand-back from a later page into one item", async () => {
    const bgm = (id: string, at: string, background: object): ConversationEntry =>
      ({ id, at, kind: "marker", marker: "background-done", background }) as ConversationEntry;
    const note = bgm("t:n:0", "2026-01-01T00:00:01Z", { source: "notification", kind: "agent", status: "completed", task_id: "a1", report_in_handback: true });
    const hand = bgm("t:h:0", "2026-01-01T00:00:02Z", { source: "handback", kind: "agent", status: "completed", task_id: "a1", label: "worker", report: "Informe final" });
    vi.spyOn(api, "conversation")
      .mockResolvedValueOnce(conv([note], { cursor: "c1", complete: false }))
      .mockResolvedValue(conv([hand], { cursor: "c2" }));
    const { container, findByRole } = render(<ConversationView session={session()} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />);
    const btn = await findByRole("button", { name: "▸" });
    await waitFor(() => expect(container.querySelectorAll(".bg-done")).toHaveLength(1));
    fireEvent.click(btn);
    expect(container.textContent).toContain("Informe final");
    expect(container.querySelectorAll(".bg-done")).toHaveLength(1);
  });

  it("shows the waiting notice while the transcript is pending", async () => {
    const { findByText } = setup(conv([], { transcript: "pending" }), session({ conversation: false }));
    expect(await findByText("Esperando la conversación de Claude…")).toBeTruthy();
  });

  it("offers the terminal when there is no conversation to show", async () => {
    const onOpenTerminal = vi.fn();
    const { findByText, getByText } = setup(conv([], { transcript: "missing" }), session({ conversation: false }), { onOpenTerminal });
    await findByText("No hay conversación que mostrar");
    fireEvent.click(getByText("Abrir terminal"));
    expect(onOpenTerminal).toHaveBeenCalled();
  });

  it("follows new entries only when the user was at the bottom", async () => {
    const spy = vi.spyOn(api, "conversation").mockResolvedValueOnce(conv([claude(1, "a")], { cursor: "c1" }));
    const { container, findByText } = render(<ConversationView session={session()} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />);
    await findByText("a");
    const log = container.querySelector(".conv-log") as HTMLDivElement;
    Object.defineProperty(log, "scrollHeight", { configurable: true, value: 1000 });
    Object.defineProperty(log, "clientHeight", { configurable: true, value: 100 });

    log.scrollTop = 0;
    fireEvent.scroll(log);
    spy.mockResolvedValueOnce(conv([claude(2, "b")], { cursor: "c2" }));
    emit("conversations");
    await findByText("b");
    expect(log.scrollTop).toBe(0);

    log.scrollTop = 900;
    fireEvent.scroll(log);
    spy.mockResolvedValueOnce(conv([claude(2, "b"), claude(3, "c")], { cursor: "c3" }));
    emit("conversations");
    await findByText("c");
    expect(log.scrollTop).toBe(1000);
  });

  it("shows the composer for a live task session and sends with sendMessage", async () => {
    const send = vi.spyOn(api, "sendMessage").mockResolvedValue(user(9, "x", "sent"));
    const { findByText, getByLabelText, getByText } = setup(conv([at("t:1", "2026-01-01T00:00:01Z", "hola")]), session({ conversation: false, label: "claude" }));
    await findByText("hola");
    fireEvent.change(getByLabelText("Mensaje"), { target: { value: "sigue" } });
    fireEvent.click(getByText("Enviar"));
    await waitFor(() => expect(send).toHaveBeenCalledWith("s1", "sigue"));
  });

  it("hides the composer for a closed session and for a shell", async () => {
    const a = setup(conv([claude(1, "x")]), session({ closed: true, live: false, conversation: false }));
    await a.findByText("x");
    expect(a.queryByLabelText("Mensaje")).toBeNull();
    cleanup();
    const b = setup(conv([claude(1, "y")]), session({ kind: "shell" as Session["kind"] }));
    await b.findByText("y");
    expect(b.queryByLabelText("Mensaje")).toBeNull();
  });

  it("shows a queued entry with its string id and removes it", async () => {
    const rm = vi.spyOn(api, "removeMessage").mockResolvedValue(null);
    const q: ConversationEntry = { id: "m-abc", at: "", kind: "user", text: "luego", state: "queued", from: "view" };
    const { findByText, getByLabelText } = setup(conv([q]), session({ state: "working", conversation: false }));
    await findByText("en cola");
    fireEvent.click(getByLabelText("Quitar mensaje"));
    expect(rm).toHaveBeenCalledWith("s1", "m-abc");
  });

  it("answers an inline card of a task session through the reply API", async () => {
    const ans = vi.spyOn(api, "answerAsk").mockResolvedValue(null as never);
    const { findByText, container } = setup(conv([card(3)]), session({ conversation: false }));
    fireEvent.click(await findByText("A"));
    const submit = [...container.querySelectorAll("button")].find((b) => /Responder|Enviar|Crear/.test(b.textContent ?? "") && b.closest(".ask"));
    if (submit) fireEvent.click(submit);
    await waitFor(() => expect(ans).toHaveBeenCalled());
  });

  it("refetches on an inbox event and shows a card answered elsewhere as answered", async () => {
    const spy = vi
      .spyOn(api, "conversation")
      .mockResolvedValueOnce(conv([card(3)], { cursor: "c1" }))
      .mockResolvedValue(conv([card(3, { outcome: "answered", answer: { "¿Cuál?": "B" } })], { cursor: "c2" }));
    const { findByText, container } = render(<ConversationView session={session()} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />);
    await findByText("¿Cuál?");
    expect(container.querySelector(".ask")).toBeTruthy();
    emit("inbox");
    await findByText("¿Cuál?: B");
    expect(container.querySelector(".ask")).toBeNull();
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it("orders mixed timestamp formats by instant, stable for equal ones", async () => {
    const first = conv([at("t:1", "2026-01-01T00:00:00.500+00:00", "b"), at("t:2", "2026-01-01T00:00:00.000Z", "a")], { cursor: "c1" });
    vi.spyOn(api, "conversation").mockResolvedValueOnce(conv([], { cursor: "c0" })).mockResolvedValue(first);
    const { container, findByText } = render(<ConversationView session={session()} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />);
    await findByText("Cuéntale tu idea…");
    emit("conversations");
    await findByText("a");
    expect([...container.querySelector(".conv-log")!.children].map((k) => k.textContent)).toEqual(["a", "b"]);
  });
});

describe("ConversationView chat mode", () => {
  class FakeSocket {
    static last: FakeSocket;
    static count = 0;
    onmessage: ((ev: { data: string }) => void) | null = null;
    onclose: ((ev: { code: number }) => void) | null = null;
    close = vi.fn();
    constructor(public url: string) {
      FakeSocket.last = this;
      FakeSocket.count++;
    }
    drop(code: number) {
      act(() => this.onclose?.({ code }));
    }
    push(m: object) {
      act(() => this.onmessage?.({ data: JSON.stringify(m) }));
    }
  }
  beforeEach(() => {
    FakeSocket.count = 0;
    vi.stubGlobal("WebSocket", FakeSocket);
  });
  afterEach(() => vi.unstubAllGlobals());

  const turnP = (started_at = "2026-01-01T00:00:00Z", step: string | null = null) => ({
    turn: { started_at, by: "user" as const, reason: null, step, last_signal_at: started_at },
    background: [],
    subagents: [],
  });

  it("shows the working indicator from the loaded progress and replaces it from the socket", async () => {
    vi.spyOn(api, "conversation").mockResolvedValue(conv([], { progress: turnP(new Date(Date.now() - 125000).toISOString()), activity: "old line" }));
    const { findByTestId, queryByText, container } = render(
      <ConversationView session={session({ runtime: "stream", state: "working" })} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />,
    );
    const ind = await findByTestId("working-indicator");
    expect(ind.textContent).toMatch(/2 min 0\d s/);
    expect(queryByText("old line")).toBeNull();
    FakeSocket.last.push({ progress: turnP(new Date().toISOString(), "Bash: ls") });
    expect(container.querySelector("[data-testid=working-indicator]")!.textContent).toContain("Bash: ls");
    FakeSocket.last.push({ progress: { turn: null, background: [], subagents: [] } });
    expect(container.querySelector("[data-testid=working-indicator]")).toBeNull();
  });

  const bgItem = (state: "running" | "completed" = "running") => ({
    task_id: "b1", tool_use_id: null, kind: "command", description: "npm run dev", state, started_at: new Date().toISOString(), ended_at: null,
  });
  const bgP = (state: "running" | "completed" = "running") => ({ turn: null, background: [bgItem(state)], subagents: [] });
  const renderBg = (progress: object | null, state: "working" | "waiting" = "working") => {
    vi.spyOn(api, "conversation").mockResolvedValue(conv([], { progress: progress as never }));
    return render(
      <ConversationView session={session({ runtime: "stream", state })} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />,
    );
  };

  it("shows the background strip from the loaded progress and the socket", async () => {
    const { findByText, queryByText } = renderBg(bgP());
    await findByText("En segundo plano");
    await findByText("npm run dev");
    FakeSocket.last.push({ progress: bgP("completed") });
    expect(queryByText("terminado")).not.toBeNull();
    FakeSocket.last.push({ progress: { turn: null, background: [], subagents: [] } });
    expect(queryByText("En segundo plano")).toBeNull();
  });

  it("keeps Detener while background runs with no turn, even when waiting", async () => {
    const stop = vi.spyOn(api, "stopSession").mockResolvedValue(null);
    const { findByText, queryByText } = renderBg(bgP(), "waiting");
    fireEvent.click(await findByText("Detener"));
    expect(stop).toHaveBeenCalledWith("s1");
    FakeSocket.last.push({ progress: bgP("completed") });
    expect(queryByText("Detener")).toBeNull();
  });

  it("sends from the composer while only background work runs, without queueing", async () => {
    const send = vi.spyOn(api, "sendMessage").mockResolvedValue(null as never);
    const { findByLabelText, findByText, queryByText } = renderBg(bgP());
    await findByText("npm run dev");
    const field = (await findByLabelText("Mensaje")) as HTMLTextAreaElement;
    fireEvent.change(field, { target: { value: "hola" } });
    fireEvent.keyDown(field, { key: "Enter", metaKey: true });
    await waitFor(() => expect(send).toHaveBeenCalledWith("s1", "hola"));
    expect(queryByText("en cola")).toBeNull();
    expect(queryByText("npm run dev")).not.toBeNull();
  });

  it("gives each Agent card its own sub-agent progress, loaded and from the socket", async () => {
    const sa = (id: string, steps: string[]) => ({
      tool_use_id: id,
      description: "d",
      state: "running" as const,
      started_at: new Date().toISOString(),
      ended_at: null,
      steps: steps.length,
      recent: steps.map((summary) => ({ tool: "Read", summary, at: "" })),
    });
    const agent = (id: string, at: string) => tool("t:" + id, at, { tool: "Agent", tool_use_id: id, summary: "Agente " + id });
    vi.spyOn(api, "conversation").mockResolvedValue(
      conv([agent("a1", "2025-01-01T00:00:01Z"), agent("a2", "2025-01-01T00:00:02Z")], {
        progress: { turn: null, background: [], subagents: [sa("a1", ["uno-a"]), sa("a2", ["uno-b", "dos-b"])] },
      }),
    );
    const { container, findAllByRole } = render(
      <ConversationView session={session({ runtime: "stream", state: "working" })} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />,
    );
    await findAllByRole("group");
    const heads = () => [...container.querySelectorAll(".tool-head")].map((h) => h.textContent!);
    expect(heads()[0]).toContain("1 paso");
    expect(heads()[0]).toContain("uno-a");
    expect(heads()[1]).toContain("2 pasos");
    expect(heads()[1]).toContain("dos-b");
    expect(heads()[0]).not.toContain("-b");
    FakeSocket.last.push({ progress: { turn: null, background: [], subagents: [sa("a1", ["uno-a", "nuevo-a"]), sa("a2", ["uno-b", "dos-b"])] } });
    expect(heads()[0]).toContain("2 pasos");
    expect(heads()[0]).toContain("nuevo-a");
    expect(heads()[1]).not.toContain("nuevo-a");
  });

  it("renders nothing for unknown markers", async () => {
    const mk = (id: string, marker: string) => ({ id, at: "2025-01-01T00:00:00Z", kind: "marker", marker }) as unknown as ConversationEntry;
    vi.spyOn(api, "conversation").mockResolvedValue(conv([mk("m1", "stopped"), mk("m3", "weird")]));
    const { findByText, container } = render(
      <ConversationView session={session({ runtime: "stream" })} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />,
    );
    await findByText("Detenida: detuviste a Claude.");
    expect(container.querySelectorAll(".conv-marker").length).toBe(1);
  });

  it("renders a background-done marker as a divider with its detail", async () => {
    const e = { id: "m1", at: "2025-01-01T00:00:00Z", kind: "marker", marker: "background-done", detail: 'Background command "sleep 5" completed' } as ConversationEntry;
    vi.spyOn(api, "conversation").mockResolvedValue(conv([e]));
    const { findByText, container } = render(
      <ConversationView session={session({ runtime: "stream" })} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />,
    );
    await findByText(/Tarea en segundo plano terminada/);
    const m = container.querySelector(".conv-marker")!;
    expect(m.textContent).toContain('Background command "sleep 5" completed');
  });

  it("renders a background-done marker with background info as a BackgroundDoneItem row", async () => {
    const e = {
      id: "m1", at: "2025-01-01T00:00:00Z", kind: "marker", marker: "background-done",
      background: { source: "notification", kind: "agent", status: "completed", label: "WP-07 update", tool_uses: 2 },
    } as ConversationEntry;
    vi.spyOn(api, "conversation").mockResolvedValue(conv([e]));
    const { findByText, container } = render(
      <ConversationView session={session({ runtime: "stream" })} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />,
    );
    await findByText(/Subagente terminado/);
    expect(container.querySelector(".bg-done")!.textContent).toContain("WP-07 update · 2 herramientas");
  });

  it("keeps the old activity line without progress", async () => {
    vi.spyOn(api, "conversation").mockResolvedValue(conv([], { activity: "old line" }));
    const { findByText, queryByTestId } = render(
      <ConversationView session={session({ runtime: "stream", state: "working" })} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />,
    );
    await findByText("old line");
    expect(queryByTestId("working-indicator")).toBeNull();
  });

  it("grows a writing bubble from deltas and replaces it with the transcript entry", async () => {
    const spy = vi.spyOn(api, "conversation").mockResolvedValue(conv([]));
    const { findByText, queryByTestId, getByTestId } = render(
      <ConversationView session={session({ runtime: "stream" })} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />,
    );
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(FakeSocket.last.url).toMatch(/\/api\/v1\/sessions\/s1\/live$/);
    FakeSocket.last.push({ message: "m1", text: "Hel" });
    expect(getByTestId("writing").textContent).toBe("Hel");
    FakeSocket.last.push({ message: "m1", text: "lo" });
    expect(getByTestId("writing").textContent).toBe("Hello");
    FakeSocket.last.push({ message: "m1", done: true });
    expect(queryByTestId("writing")).not.toBeNull();
    spy.mockResolvedValue(conv([at("t:uuid1:0", "2025-01-01T00:00:00Z", "Hello")], { cursor: "1" }));
    emit("conversations");
    await findByText("Hello", { selector: ".conv-claude:not(.conv-writing) *" });
    await waitFor(() => expect(queryByTestId("writing")).toBeNull());
  });

  it("replaces the bubble exactly when the claude entry of its message arrives", async () => {
    const spy = vi.spyOn(api, "conversation").mockResolvedValue(conv([]));
    const { findByText, queryByTestId, getByTestId } = render(
      <ConversationView session={session({ runtime: "stream" })} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />,
    );
    await waitFor(() => expect(spy).toHaveBeenCalled());
    FakeSocket.last.push({ message: "m1", text: "Hel" });
    FakeSocket.last.push({ message: "m1", done: true });
    // An entry of another message (even a new one) does not take the bubble's place.
    spy.mockResolvedValue(conv([{ ...at("t:u0:0", "2025-01-01T00:00:00Z", "Other"), message_id: "m0" } as ConversationEntry], { cursor: "1" }));
    emit("conversations");
    await findByText("Other");
    expect(getByTestId("writing").textContent).toBe("Hel");
    spy.mockResolvedValue(
      conv(
        [
          { ...at("t:u0:0", "2025-01-01T00:00:00Z", "Other"), message_id: "m0" } as ConversationEntry,
          { ...at("t:u1:0", "2025-01-01T00:00:01Z", "Hello"), message_id: "m1" } as ConversationEntry,
        ],
        { cursor: "2" },
      ),
    );
    emit("conversations");
    await findByText("Hello", { selector: ".conv-claude:not(.conv-writing) *" });
    await waitFor(() => expect(queryByTestId("writing")).toBeNull());
  });

  it("does not open the socket on a pty session", async () => {
    vi.spyOn(api, "conversation").mockResolvedValue(conv([]));
    FakeSocket.last = undefined as never;
    render(<ConversationView session={session()} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />);
    expect(FakeSocket.last).toBeUndefined();
  });

  it("does not open the socket on a chat session that is not running", async () => {
    const spy = vi.spyOn(api, "conversation").mockResolvedValue(conv([]));
    render(<ConversationView session={session({ runtime: "stream", live: false })} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />);
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(FakeSocket.count).toBe(0);
  });

  it("drops the partial text and reloads on a reset", async () => {
    const spy = vi.spyOn(api, "conversation").mockResolvedValue(conv([]));
    const { getByTestId, queryByTestId } = render(
      <ConversationView session={session({ runtime: "stream" })} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />,
    );
    await waitFor(() => expect(spy).toHaveBeenCalled());
    FakeSocket.last.push({ message: "m1", text: "Hel" });
    expect(getByTestId("writing").textContent).toBe("Hel");
    const calls = spy.mock.calls.length;
    FakeSocket.last.push({ reset: true });
    expect(queryByTestId("writing")).toBeNull();
    await waitFor(() => expect(spy.mock.calls.length).toBeGreaterThan(calls));
  });

  describe("reconnection", () => {
    const mount = () => render(<ConversationView session={session({ runtime: "stream" })} onOpenTerminal={() => {}} onOpenTask={() => {}} onReopen={() => {}} />);
    beforeEach(() => {
      vi.spyOn(api, "conversation").mockResolvedValue(conv([]));
      vi.useFakeTimers({ shouldAdvanceTime: true });
    });
    afterEach(() => vi.useRealTimers());

    it("reconnects with a backoff after an abnormal close", async () => {
      mount();
      await waitFor(() => expect(FakeSocket.count).toBe(1));
      FakeSocket.last.drop(1006);
      act(() => void vi.advanceTimersByTime(1000));
      expect(FakeSocket.count).toBe(2);
      FakeSocket.last.drop(1006);
      act(() => void vi.advanceTimersByTime(1000));
      expect(FakeSocket.count).toBe(2);
      act(() => void vi.advanceTimersByTime(1000));
      expect(FakeSocket.count).toBe(3);
    });

    it("handles a 4409 close quietly: no reconnect, no error", async () => {
      const { container } = mount();
      await waitFor(() => expect(FakeSocket.count).toBe(1));
      FakeSocket.last.drop(4409);
      act(() => void vi.advanceTimersByTime(30000));
      expect(FakeSocket.count).toBe(1);
      expect(container.querySelector(".error")).toBeNull();
    });

    it("does not reconnect after unmount", async () => {
      const { unmount } = mount();
      await waitFor(() => expect(FakeSocket.count).toBe(1));
      const first = FakeSocket.last;
      first.drop(1006);
      unmount();
      act(() => void vi.advanceTimersByTime(30000));
      expect(FakeSocket.count).toBe(1);
    });
  });
});

