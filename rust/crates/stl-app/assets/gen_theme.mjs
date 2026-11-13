// Regenerates the token block of src/theme.rs from web/css/app.css:
//   node rust/crates/stl-app/assets/gen_theme.mjs
import { readFileSync, writeFileSync } from 'node:fs';
const css = readFileSync(new URL('../../../../web/css/app.css', import.meta.url), 'utf8');
const block = (re) => css.match(re)[1];
const decls = (b) => [...b.replace(/\/\*[\s\S]*?\*\//g, '').matchAll(/--([a-z0-9-]+)\s*:\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]);
const dark = new Map(decls(block(/:root\s*\{([\s\S]*?)\n\}/)));
const light = new Map([...dark, ...decls(block(/html\[data-theme=light\]\s*\{([\s\S]*?)\n\}/))]);
const snake = (n) => n.replace(/-/g, '_');
const col = (v) => {
  let m = v.match(/^#([0-9a-f]{6})$/i);
  if (m) { const h = m[1]; return `Color32::from_rgb(0x${h.slice(0, 2)}, 0x${h.slice(2, 4)}, 0x${h.slice(4)})`; }
  m = v.match(/^rgba\((\d+),\s*(\d+),\s*(\d+),\s*([\d.]+)\)$/);
  if (m) return `Color32::from_rgba_unmultiplied(${m[1]}, ${m[2]}, ${m[3]}, ${Math.round(parseFloat(m[4]) * 255)})`;
  throw new Error('colour? ' + v);
};
const shadow = (v) => {
  const m = v.match(/^0 (\d+)px (\d+)px (rgba\(.*\))$/);
  return `Shadow { offset: [0, ${m[1]}], blur: ${m[2]}, spread: 0, color: ${col(m[3])} }`;
};
const NON = new Set(['shadow', 'shadow-lg', 'radius', 'radius-sm', 'font']);
const names = [...dark.keys()].filter((n) => !NON.has(n));
const fn = (map) => `Tokens {\n${names.map((n) => `            ${snake(n)}: ${col(map.get(n))},`).join('\n')}
            shadow: ${shadow(map.get('shadow'))},
            shadow_lg: ${shadow(map.get('shadow-lg'))},
            radius: ${parseFloat(map.get("radius")).toFixed(1)},
            radius_sm: ${parseFloat(map.get("radius-sm")).toFixed(1)},
        }`;
const gen = `// BEGIN GENERATED (assets/gen_theme.mjs from web/css/app.css; do not edit by hand)
/// Every colour token of web/css/app.css, same names in snake_case.
#[derive(Clone, Debug, PartialEq)]
pub struct Tokens {
${names.map((n) => `    pub ${snake(n)}: Color32,`).join('\n')}
    /// \`--shadow\` (menus, panels) and \`--shadow-lg\` (dialogs).
    pub shadow: Shadow,
    pub shadow_lg: Shadow,
    /// \`--radius\` and \`--radius-sm\`, in points (CSS px).
    pub radius: f32,
    pub radius_sm: f32,
}

/// CSS names of the colour tokens, in the same order as the fields.
pub const COLOUR_NAMES: &[&str] = &[${names.map((n) => `"${n}"`).join(', ')}];

impl Tokens {
    /// A colour token by its CSS name (\`"panel-2"\`, without the leading \`--\`).
    pub fn colour(&self, name: &str) -> Option<Color32> {
        Some(match name {
${names.map((n) => `            "${n}" => self.${snake(n)},`).join('\n')}
            _ => return None,
        })
    }
}

pub fn tokens(dark: bool) -> Tokens {
    if dark {
        ${fn(dark)}
    } else {
        ${fn(light)}
    }
}
// END GENERATED
`;
const p = new URL('../src/theme.rs', import.meta.url);
let src = readFileSync(p, 'utf8');
src = src.replace(/\/\/ BEGIN GENERATED[\s\S]*?\/\/ END GENERATED\n/, () => gen);
writeFileSync(p, src);
console.log(names.length, 'colour tokens');
