-- Configurable task start: projects registered before this keep today's behaviour
-- (the "Spec Kit (/feature)" preset, review message in the app's language).
-- Tasks get the message they start with, and `feature` becomes `project`.
UPDATE projects SET settings = json_set(settings, '$.workflow', json(
  CASE WHEN (SELECT value FROM settings WHERE key = 'pref:language') IN ('"en"', 'en')
    THEN '{"kind":"phased","plan":"/feature {task}","implement":"/orchestrate {spec_dir}","review":"/review the branch of this task. Then apply the confirmed findings, update the docs they made stale and commit. Do not push or open a PR."}'
    ELSE '{"kind":"phased","plan":"/feature {task}","implement":"/orchestrate {spec_dir}","review":"/review de la rama de esta tarea. Después aplica los hallazgos confirmados, actualiza la documentación que hayan dejado obsoleta y haz commit. No hagas push ni abras una PR."}'
  END))
  WHERE json_extract(settings, '$.workflow') IS NULL;

ALTER TABLE tasks ADD COLUMN start_message TEXT;

UPDATE projects SET workflow = 'project' WHERE workflow = 'feature';

UPDATE projects SET workflow = 'prompt' WHERE workflow NOT IN ('project', 'prompt', 'none');

UPDATE tasks SET workflow = 'project' WHERE workflow = 'feature';

UPDATE tasks SET workflow = 'prompt' WHERE workflow NOT IN ('project', 'prompt', 'none');
