// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, type Conversation, type ConversationEntry, type Session } from "../api";
import { setLanguage } from "../i18n";
import { ConversationView } from "./ConversationView";

beforeEach(() => setLanguage("es"));

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
    fireEvent.click(document.querySelector(".btn-p") as HTMLButtonElement);
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
});
