function normalize(value) {
  return String(value || '').trim().toLowerCase();
}

function moduleConfig(ctx) {
  return typeof ctx.moduleConfig === 'function' && ctx.moduleConfig() ? ctx.moduleConfig() : {};
}

function configString(config, key, fallback) {
  const value = typeof config[key] === 'string' ? config[key].trim() : '';
  return value || fallback;
}

function configAliases(config) {
  const aliases = Array.isArray(config.aliases) ? config.aliases : ['color', 'cp', 'picker', 'pick'];
  return aliases.map(normalize).filter(Boolean);
}

function quote(value) {
  const text = String(value || '');
  if (text.startsWith('"') && text.endsWith('"')) return text;
  return `"${text.replace(/"/g, '\\"')}"`;
}

function joinPath(base, relative) {
  const cleanBase = String(base || '').replace(/[\\/]+$/, '');
  const cleanRelative = String(relative || '').replace(/^[\\/]+/, '');
  return `${cleanBase}/${cleanRelative}`;
}

function parseFormat(raw, fallback) {
  const value = normalize(raw);
  return ['hex', 'rgb', 'hsl', 'all'].includes(value) ? value : fallback;
}

export default function createModule() {
  return {
    onQueryChange(query, ctx) {
      const input = String(query || '').trim();
      const parts = input.split(/\s+/).filter(Boolean);
      const command = normalize(parts[0]);
      const config = moduleConfig(ctx);
      const aliases = configAliases(config);

      if (!aliases.includes(command)) {
        ctx.clearInputAccessory();
        return;
      }

      const defaultFormat = parseFormat(configString(config, 'defaultFormat', 'hex'), 'hex');
      const format = parseFormat(parts[1], defaultFormat);
      const moduleDir = typeof ctx.moduleDir === 'function' ? ctx.moduleDir() : '';
      const helper = joinPath(moduleDir, configString(config, 'helper', 'bin/color-picker.exe'));
      const target = `${quote(helper)} --format ${format}`;

      ctx.setInputAccessory({
        text: `pick screen color -> ${format.toUpperCase()}`,
        kind: 'hint',
        priority: 80
      });

      ctx.replaceItems([{
        id: `color-picker::${format}`,
        title: 'Pick screen color',
        subtitle: `Copy ${format.toUpperCase()} to clipboard`,
        source: 'color-picker',
        target,
        badge: format.toUpperCase(),
        hint: helper
      }]);
    }
  };
}
