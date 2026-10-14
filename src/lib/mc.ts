// Minecraft knowledge the UI needs: server software catalog, formatting codes, game constants.

export interface SoftwareInfo {
  id: string;
  label: string;
  tagline: string;
  /** What add-ons it loads */
  content: 'mods' | 'plugins' | 'none';
  /** Tile colours */
  color: string;
  bg: string;
  description: string;
  goodFor: string[];
  site: string;
}

export const SOFTWARE: SoftwareInfo[] = [
  {
    id: 'vanilla',
    label: 'Vanilla',
    tagline: 'Official Mojang server',
    content: 'none',
    color: '#4ade80',
    bg: 'linear-gradient(135deg,#14532d,#166534)',
    description: 'The untouched server straight from Mojang – every release and snapshot back to 2012. Datapacks only.',
    goodFor: ['Pure survival', 'Snapshots', 'Datapacks'],
    site: 'https://www.minecraft.net/download/server',
  },
  {
    id: 'paper',
    label: 'Paper',
    tagline: 'Fast, plugin-ready',
    content: 'plugins',
    color: '#e2e8f0',
    bg: 'linear-gradient(135deg,#334155,#1e293b)',
    description: 'High-performance fork with Bukkit/Spigot/Paper plugin support and many exploit and lag fixes. The best default for most servers.',
    goodFor: ['Plugins', 'Performance', 'Public servers'],
    site: 'https://papermc.io',
  },
  {
    id: 'purpur',
    label: 'Purpur',
    tagline: 'Paper + endless tweaks',
    content: 'plugins',
    color: '#d8b4fe',
    bg: 'linear-gradient(135deg,#581c87,#3b0764)',
    description: 'A Paper fork with hundreds of extra gameplay options (rideable mobs, configurable everything) while staying plugin compatible.',
    goodFor: ['Plugins', 'Custom gameplay', 'SMPs'],
    site: 'https://purpurmc.org',
  },
  {
    id: 'folia',
    label: 'Folia',
    tagline: 'Multithreaded regions',
    content: 'plugins',
    color: '#86efac',
    bg: 'linear-gradient(135deg,#365314,#1a2e05)',
    description: 'Paper with regionised multithreading for very large player counts. Only Folia-aware plugins work.',
    goodFor: ['Huge player counts', 'Many cores'],
    site: 'https://papermc.io/software/folia',
  },
  {
    id: 'fabric',
    label: 'Fabric',
    tagline: 'Lightweight mods',
    content: 'mods',
    color: '#fde68a',
    bg: 'linear-gradient(135deg,#78350f,#451a03)',
    description: 'Lightweight, fast-updating mod loader – first on new Minecraft versions. Great optimisation mods (Lithium, C2ME…).',
    goodFor: ['Mods', 'New versions', 'Optimisation'],
    site: 'https://fabricmc.net',
  },
  {
    id: 'quilt',
    label: 'Quilt',
    tagline: 'Fabric-compatible',
    content: 'mods',
    color: '#c4b5fd',
    bg: 'linear-gradient(135deg,#4c1d95,#2e1065)',
    description: 'Community fork of Fabric that loads most Fabric mods plus Quilt mods.',
    goodFor: ['Mods', 'Fabric mods'],
    site: 'https://quiltmc.org',
  },
  {
    id: 'forge',
    label: 'Forge',
    tagline: 'The classic modding API',
    content: 'mods',
    color: '#fdba74',
    bg: 'linear-gradient(135deg,#7c2d12,#431407)',
    description: 'The long-running mod loader behind most classic modpacks (1.7.10 → today).',
    goodFor: ['Big modpacks', 'Older versions'],
    site: 'https://files.minecraftforge.net',
  },
  {
    id: 'neoforge',
    label: 'NeoForge',
    tagline: 'Modern Forge successor',
    content: 'mods',
    color: '#fca5a5',
    bg: 'linear-gradient(135deg,#7f1d1d,#450a0a)',
    description: 'The community fork that most modern Forge-style mods moved to (1.20.2+).',
    goodFor: ['Modern modpacks', 'Create, Mekanism…'],
    site: 'https://neoforged.net',
  },
];

export function softwareInfo(id: string | undefined): SoftwareInfo {
  return SOFTWARE.find((s) => s.id === id) ?? SOFTWARE[0];
}

/** Loader label for a build list ("Build", "Loader", "Forge version"). */
export function loaderNoun(software: string): string {
  switch (software) {
    case 'paper':
    case 'purpur':
    case 'folia':
      return 'Build';
    case 'fabric':
    case 'quilt':
      return 'Loader version';
    case 'forge':
      return 'Forge version';
    case 'neoforge':
      return 'NeoForge version';
    default:
      return '';
  }
}

// ---------------------------------------------------------------------- formatting codes

export const MC_COLORS: Record<string, { name: string; hex: string }> = {
  '0': { name: 'Black', hex: '#000000' },
  '1': { name: 'Dark Blue', hex: '#0000AA' },
  '2': { name: 'Dark Green', hex: '#00AA00' },
  '3': { name: 'Dark Aqua', hex: '#00AAAA' },
  '4': { name: 'Dark Red', hex: '#AA0000' },
  '5': { name: 'Dark Purple', hex: '#AA00AA' },
  '6': { name: 'Gold', hex: '#FFAA00' },
  '7': { name: 'Gray', hex: '#AAAAAA' },
  '8': { name: 'Dark Gray', hex: '#555555' },
  '9': { name: 'Blue', hex: '#5555FF' },
  a: { name: 'Green', hex: '#55FF55' },
  b: { name: 'Aqua', hex: '#55FFFF' },
  c: { name: 'Red', hex: '#FF5555' },
  d: { name: 'Light Purple', hex: '#FF55FF' },
  e: { name: 'Yellow', hex: '#FFFF55' },
  f: { name: 'White', hex: '#FFFFFF' },
};

export const MC_STYLES: Record<string, string> = { l: 'Bold', o: 'Italic', n: 'Underline', m: 'Strikethrough', k: 'Obfuscated', r: 'Reset' };

export interface Run {
  text: string;
  color: string;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strike: boolean;
  obf: boolean;
}

/** Split `§`-formatted text into styled runs (colour resets styles, like the game). */
export function parseFormatted(text: string, base = '#AAAAAA'): Run[] {
  const runs: Run[] = [];
  let cur: Run = { text: '', color: base, bold: false, italic: false, underline: false, strike: false, obf: false };
  const push = () => {
    if (cur.text) runs.push(cur);
    cur = { ...cur, text: '' };
  };
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if ((c === '§' || c === '&') && i + 1 < text.length) {
      const code = text[i + 1].toLowerCase();
      if (MC_COLORS[code]) {
        push();
        cur = { text: '', color: MC_COLORS[code].hex, bold: false, italic: false, underline: false, strike: false, obf: false };
        i++;
        continue;
      }
      if (code in MC_STYLES) {
        push();
        if (code === 'l') cur.bold = true;
        else if (code === 'o') cur.italic = true;
        else if (code === 'n') cur.underline = true;
        else if (code === 'm') cur.strike = true;
        else if (code === 'k') cur.obf = true;
        else cur = { text: '', color: base, bold: false, italic: false, underline: false, strike: false, obf: false };
        i++;
        continue;
      }
    }
    cur.text += c;
  }
  push();
  return runs;
}

export function stripFormatting(s: string): string {
  return s.replace(/[§&][0-9a-fk-or]/gi, '');
}

/** Turn `&a` shorthand into real `§` codes (what the server needs). */
export function ampToSection(s: string): string {
  return s.replace(/&([0-9a-fk-or])/gi, '§$1');
}

export const GAMEMODES = ['survival', 'creative', 'adventure', 'spectator'];
export const DIFFICULTIES = ['peaceful', 'easy', 'normal', 'hard'];
export const GAME_TYPE_LABEL = ['Survival', 'Creative', 'Adventure', 'Spectator'];
export const DIFFICULTY_LABEL = ['Peaceful', 'Easy', 'Normal', 'Hard'];

/** Player head image (works for names and UUIDs). */
export function headUrl(nameOrUuid: string, size = 32): string {
  return `https://mc-heads.net/avatar/${encodeURIComponent(nameOrUuid)}/${size}`;
}

/** Vanilla commands for console autocomplete (plus common plugin ones). */
export const COMMANDS = [
  'advancement', 'attribute', 'ban', 'ban-ip', 'banlist', 'bossbar', 'clear', 'clone', 'damage', 'data', 'datapack', 'debug', 'defaultgamemode', 'deop',
  'difficulty', 'effect', 'enchant', 'execute', 'experience', 'fill', 'fillbiome', 'forceload', 'function', 'gamemode', 'gamerule', 'give', 'help', 'item',
  'jfr', 'kick', 'kill', 'list', 'locate', 'loot', 'me', 'msg', 'op', 'pardon', 'pardon-ip', 'particle', 'perf', 'place', 'playsound', 'random', 'recipe',
  'reload', 'return', 'ride', 'rotate', 'save-all', 'save-off', 'save-on', 'say', 'schedule', 'scoreboard', 'seed', 'setblock', 'setidletimeout',
  'setworldspawn', 'spawnpoint', 'spectate', 'spreadplayers', 'stop', 'stopsound', 'summon', 'tag', 'team', 'teammsg', 'teleport', 'tell', 'tellraw',
  'tick', 'time', 'title', 'tm', 'tp', 'transfer', 'trigger', 'w', 'weather', 'whitelist', 'worldborder', 'xp',
  'tps', 'mspt', 'plugins', 'version', 'timings', 'spark', 'forge', 'neoforge',
];

export const GAMERULES = [
  'keepInventory', 'doDaylightCycle', 'doWeatherCycle', 'doMobSpawning', 'mobGriefing', 'doFireTick', 'playersSleepingPercentage', 'naturalRegeneration',
  'doImmediateRespawn', 'announceAdvancements', 'showDeathMessages', 'randomTickSpeed', 'spawnRadius', 'doInsomnia', 'doPatrolSpawning', 'doTraderSpawning',
  'fallDamage', 'fireDamage', 'drowningDamage', 'freezeDamage', 'pvp', 'locatorBar', 'commandBlockOutput', 'sendCommandFeedback', 'logAdminCommands',
];
