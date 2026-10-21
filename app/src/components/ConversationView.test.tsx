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
  activity: null,
  needs_terminal: false,
  ...over,
});

const user = (id: number, text: string, state: "queued" | "sent" | "delivered" = "delivered"): ConversationEntry => ({ id, at: "", kind: "user", text, state, from: "view" });
const claude = (id: number, text: string): ConversationEntry => ({ id, at: "", kind: "claude", text });
const card = (id: number, over: object = {}): ConversationEntry => ({
  id,
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
    expect(rm).toHaveBeenCalledWith("s1", 4);
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
});
