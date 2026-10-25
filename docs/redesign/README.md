# Capturas del rediseño

Capturas de todas las pantallas de la UI web, para comparar antes y después del rediseño.

- `antes/`: la UI en el tag local `ui-v1` (commit `793e4cf`, versión 0.11.0).
- `despues/`: la misma batería con el rediseño aplicado.

Cada pantalla sale dos veces, `<Pantalla>-oscuro.png` y `<Pantalla>-claro.png` (23 por tema),
con los nombres de los tableros del lienzo del rediseño. Ventana de 1440×900 salvo
`Tablero1280` y `Tarea1280` (1280×800) y `Jira1024` (1024×700). UI en español, zona horaria
Europe/Madrid, animaciones y cursor desactivados.

## Regenerarlas

El script es `app/e2e/screens.spec.ts` (Playwright). Arranca su propio daemon aislado y su
Vite (`app/e2e/setup.ts`, con el Claude, el `gh` y el Jira falsos de los e2e), así que no toca
la app instalada (7420) ni las previews, y no deja nada corriendo. Sin `DELILAH_SCREENS` se salta.

```bash
cargo build -p delilah-daemon -p delilah-cli -p delilah-e2e
cd app && DELILAH_SCREENS=despues npx playwright test screens   # o antes
```

Ejecútalo solo (con el filtro `screens`): necesita un daemon vacío para que las claves salgan
igual (`LIBRE-1`…`LIBRE-8`, `PROJ-1246`), y falla si encuentra proyectos o tareas.
Para volver a la UI de antes: `git checkout ui-v1`.

## Datos

Proyectos `signal` (repo) y `delilah` (parent, con submódulos), sobre los repos de prueba del e2e.

| Tarea | Estado |
|---|---|
| LIBRE-1/2/3 | backlog |
| LIBRE-4 | hecha, con PR #12 fusionada |
| LIBRE-5 «Chat: tarjetas de permiso en vivo» | modo chat con una tarjeta de permiso pendiente, cambios en README.md, una terminal y la PR #14 (CI fallido, cambios pedidos, un comentario) |
| LIBRE-6 | en ejecución (agente de terminal) |
| LIBRE-7 | pausada por el límite de uso |
| LIBRE-8 | setup fallido |
| PROJ-1246 | ticket de Jira en `delilah`, te espera |

Jira conectado con los datos de ejemplo (modo `sample`), con PROJ-1242 seleccionado.

## Lo que no sale igual que en las maquetas, y por qué

- **LIBRE-5 aparece «En ejecución», no «Te espera»**: con una tarjeta de permiso pendiente el
  daemon deja la tarea en ejecución (la espera se ve en la tarjeta y en la Bandeja).
  La tarea que «te espera» es PROJ-1246.
- **Tarea con estado «Fallida»**: no hay un camino con los falsos de los e2e para llegar a él; no se captura.
- **Logs**: en la app nativa se abren desde Ayuda › Ver logs; en la web no hay botón, así
  que el script lanza el mismo evento (`delilah:show-logs`).
- **Jira-conectar** es el formulario de un Jira sin configurar (se captura antes de conectarlo).
- **Horas relativas** («hace N min», «Pausada hasta las HH:MM», el historial de la tarea) y
  los logs dependen de cuándo se ejecuta, y las rutas de los proyectos (Proyectos, vista del
  proyecto, tarjeta de checkout) son carpetas temporales: cambian entre ejecuciones.
- Lo nativo de macOS (notificaciones, menú, Dock, ⌘P global) no se ve en la UI web.
