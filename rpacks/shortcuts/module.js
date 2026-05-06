import fs from 'node:fs';

const USER_SHORTCUTS_PATH = 'modules/shortcuts/shortcuts.user.json';
let pendingShortcutItem = null;
let cachedUserShortcuts = null;
let cachedConfigShortcuts = null;

function normalize(value) {
  return String(value || '').trim().toLowerCase();
}

function displayTarget(shortcut) {
  return String(shortcut.target || '').trim();
}

function launchTarget(shortcut) {
  const target = displayTarget(shortcut);
  if (!target) return '';
  if (target.includes(' ') && !(target.startsWith('"') && target.endsWith('"'))) {
    return `"${target}"`;
  }
  return target;
}

function cueFor(shortcut) {
  const alias = String(shortcut.alias || '').trim();
  if (alias) return alias;
  return String(shortcut.key || '').trim();
}

function sanitizeShortcut(shortcut) {
  if (!shortcut || typeof shortcut.title !== 'string' || typeof shortcut.target !== 'string') return null;
  const normalized = {
    key: String(shortcut.key || '').trim(),
    alias: String(shortcut.alias || '').trim(),
    title: shortcut.title.trim(),
    target: displayTarget(shortcut)
  };
  if (!normalized.title || !normalized.target) return null;
  if (!normalized.key && !normalized.alias) return null;
  return normalized;
}

function readUserShortcuts() {
  try {
    if (!fs.existsSync(USER_SHORTCUTS_PATH)) return [];
    const parsed = JSON.parse(fs.readFileSync(USER_SHORTCUTS_PATH, 'utf8'));
    const shortcuts = parsed && Array.isArray(parsed.shortcuts) ? parsed.shortcuts : [];
    return shortcuts.map(sanitizeShortcut).filter(Boolean);
  } catch (_) {
    return [];
  }
}

function loadUserShortcuts() {
  if (!cachedUserShortcuts) {
    cachedUserShortcuts = readUserShortcuts();
  }
  return cachedUserShortcuts;
}

function saveUserShortcut(shortcut) {
  const existing = loadUserShortcuts();
  const alias = normalize(shortcut.alias);
  const key = normalize(shortcut.key);
  const next = existing.filter((entry) => {
    if (alias && normalize(entry.alias) === alias) return false;
    if (key && normalize(entry.key) === key) return false;
    return true;
  });
  next.push(shortcut);
  fs.writeFileSync(USER_SHORTCUTS_PATH, `${JSON.stringify({ shortcuts: next }, null, 2)}\n`, 'utf8');
  cachedUserShortcuts = next;
}

function loadConfigShortcuts(ctx) {
  if (!cachedConfigShortcuts) {
    const config = typeof ctx.moduleConfig === 'function' ? ctx.moduleConfig() : null;
    const configured = config && Array.isArray(config.shortcuts) ? config.shortcuts : [];
    cachedConfigShortcuts = configured.map(sanitizeShortcut).filter(Boolean);
  }
  return cachedConfigShortcuts;
}

function loadShortcuts(ctx) {
  return loadConfigShortcuts(ctx).concat(loadUserShortcuts());
}

function matchesShortcut(input, shortcut) {
  const query = normalize(input);
  if (!query) return false;
  return normalize(shortcut.key) === query || normalize(shortcut.alias) === query;
}

function shortcutId(shortcut) {
  const slug = shortcut.title.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
  return `shortcuts::${slug || 'shortcut'}`;
}

function shortcutItem(shortcut) {
  const cue = cueFor(shortcut);
  const target = displayTarget(shortcut);
  return {
    id: shortcutId(shortcut),
    title: shortcut.title,
    subtitle: target,
    source: 'shortcuts',
    target: launchTarget(shortcut),
    badge: cue,
    hint: target
  };
}

function itemToPendingShortcut(item) {
  if (!item || typeof item !== 'object') return null;
  const title = String(item.title || '').trim();
  const target = String(item.target || item.subtitle || '').trim();
  if (!title || !target) return null;
  return { title, target };
}

function isBindKey(event) {
  return Boolean(event && event.ctrl && !event.alt && !event.meta && normalize(event.key) === 'b');
}

export default function createModule() {
  return {
    onQueryChange(query, ctx) {
      const input = String(query || '').trim();
      const match = loadShortcuts(ctx).find((shortcut) => matchesShortcut(input, shortcut));

      if (!match) {
        ctx.clearInputAccessory();
        return;
      }

      ctx.setInputAccessory({
        text: `shortcut: ${match.title}`,
        kind: 'hint',
        priority: 90
      });
      ctx.replaceItems([shortcutItem(match)]);
    },

    onKey(event, ctx) {
      if (!isBindKey(event)) return;

      const pending = itemToPendingShortcut(ctx.selectedItem());
      if (!pending) {
        ctx.setInputAccessory({
          text: 'shortcut bind: no selected item',
          kind: 'warning',
          priority: 95
        });
        return;
      }

      pendingShortcutItem = pending;
      ctx.setQuery('/shortcuts::bind ');
      ctx.setInputAccessory({
        text: `binding ${pending.title}: type alias and press Enter`,
        kind: 'info',
        priority: 95
      });
    },

    onCommand(command, args, ctx) {
      if (normalize(command) !== 'bind') return;

      const alias = String(Array.isArray(args) ? args[0] || '' : '').trim();
      if (!alias) {
        ctx.setInputAccessory({
          text: 'shortcut bind: missing alias',
          kind: 'warning',
          priority: 95
        });
        return;
      }

      if (!pendingShortcutItem) {
        ctx.setInputAccessory({
          text: 'shortcut bind: select an item and press Ctrl+B first',
          kind: 'warning',
          priority: 95
        });
        return;
      }

      saveUserShortcut({
        key: '',
        alias,
        title: pendingShortcutItem.title,
        target: pendingShortcutItem.target
      });

      ctx.setInputAccessory({
        text: `shortcut added: ${alias} -> ${pendingShortcutItem.title}`,
        kind: 'success',
        priority: 100
      });
      pendingShortcutItem = null;
    }
  };
}
