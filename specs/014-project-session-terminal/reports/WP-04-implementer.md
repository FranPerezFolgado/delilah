## WP-04 — done
Tasks:    T006 ✓, T007 ✓, T008 ✓, T009 ✓ (no other test needed fixing)
Files:    app/src/components/TaskScreen.tsx — project sessions render SessionViewSwitch mode="view"; chooseView calls rememberView only when !project; comments updated
          app/src/components/TaskScreen.test.tsx — replaced "no switch" test with 5 project/idea tests, added task confirm-dialog guard; ConversationView mock exposes an onOpenTerminal button
Tests:    flips terminal/chat view-only; enabled while working; per-session view; chat's open-terminal keeps Terminal pressed; idea conversation opens in chat with switch; task session opens runtime confirm. Failed first (5), then pass. tsc + full vitest: 1358 passed.
Notes:    In test (d) inactive sessions stay mounted (hidden), so it asserts the switch's pressed state after selecting tab 2 rather than absence of the conversation node.
