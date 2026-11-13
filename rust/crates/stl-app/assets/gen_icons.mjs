// Regenerates assets/icons.json from web/js/icons.js: node rust/crates/stl-app/assets/gen_icons.mjs
import { ICONS } from '../../../../web/js/icons.js';
import { writeFileSync } from 'node:fs';
const out = {};
for (const k of Object.keys(ICONS).sort()) out[k] = ICONS[k];
writeFileSync(new URL('./icons.json', import.meta.url), JSON.stringify(out, null, 1) + '\n');
console.log(Object.keys(out).length, 'icons');
