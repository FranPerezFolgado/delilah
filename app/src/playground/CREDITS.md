# Playground credits

The playground's pixel art, palette and colour logic come from **pixtuoid**, a
terminal pixel-art office for coding agents.

- Source: https://github.com/ivanwang123/pixtuoid
- Commit: `90320b132ef0f5e844685e711f0820652c354dca`
- Author: Ivan Wang
- Licence: MIT (full text below)

## What is used

- The 1x sprites (characters, desk pieces, lounge furniture, decor, icons),
  ported by `scripts/port-pixtuoid.py` into `art/pixtuoid.ts`. The `@4x` art is
  not used.
- The colour palette and the ramp table (derived shades), and the OKLab ramp
  step (`Rgb::ramp`), re-implemented in TypeScript; `art/ramp.fixtures.ts`
  holds the expected shades.
- The outfit table (shirt/pants colours) and the seed hash used to pick an
  outfit from a name.
- The room colours (wall, wall trim, warm carpet) of its default theme, in
  `art/office.ts`, and the way it composes a room: a wall band across the top,
  wall decor on it, a sitter behind a viewer-facing desk.
- The monitor glow colours and the token-tier logic (the paper tower).

`art/pixtuoid.ts` and `art/ramp.fixtures.ts` are generated: do not edit them,
rerun the script against a checkout of the commit above.

agent-flow was only a reference for the design; nothing was copied from it.
No other third-party art is used.

## Licence of pixtuoid

MIT License

Copyright (c) 2026 Ivan Wang

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
