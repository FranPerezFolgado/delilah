---
name: task-from-idea
description: Use when the user wants a task created from an idea, or from the current conversation ("make a task from this", "turn this idea into a task", "crea una tarea"). Runs a short mini-analysis, writes a brief and proposes the task with a confirmation card.
---

# Task from idea

You turn the user's idea into a Delilah task. Answer the user in the language
they write in. The brief itself always uses the Spanish section headings below.

Follow this protocol exactly.

1. **Ask with AskUserQuestion, never in prose.** Whenever you need the user to
   choose between options, use the AskUserQuestion tool (question cards). Do not
   list options in plain text.

2. **Read the code only as far as a mini-analysis needs.** Spend minutes, not a
   spec: find the areas the idea likely touches, then stop.

3. **Write the brief in Markdown** with these four sections, with these exact
   headings:
   - `## Idea`: the idea in the user's own words.
   - `## Resumen`: a short summary of the feature.
   - `## Zonas afectadas`: the paths in the code it likely touches.
   - `## Preguntas abiertas`: the open questions.

4. **Propose with ONE AskUserQuestion** per task, using exactly:
   - `header: "Nueva tarea"`
   - `question: <the task title>`
   - three options: `Crear` (with `preview: <the full brief>`), `Cambiar algo`
     and `Descartar`.

   Create nothing until the user picks `Crear`. If they pick `Cambiar algo`,
   find out what to change, update the brief and propose again. If they pick
   `Descartar`, drop it. If the conversation holds several ideas, make one
   proposal card per task.

5. **Read the answer to `Crear`.**
   - `Crear — creada <KEY> …`: tell the user the task key.
   - `Crear — no se pudo crear`, or a bare `Crear` (the daemon was away): tell
     the user the task was NOT created and offer to retry.

6. **Never create tasks by any other means** than the confirmation card above,
   and never use ExitPlanMode.
