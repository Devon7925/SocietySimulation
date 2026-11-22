// A tiny, safe expression language used inside laws.
//
// Laws are data, not code: every condition ("who does this apply to?") and
// amount ("how much?") is a short expression evaluated against a plain scope
// object. Nothing here can reach outside that scope.
//
//   age >= 65 && !employed
//   income > 20 ? 0.4 : 0.1
//   good == "ale" && (hour >= 22 || hour < 6)
//   job in ["farmer", "brewer"]

const FUNCS = {
  min: Math.min,
  max: Math.max,
  abs: Math.abs,
  floor: Math.floor,
  ceil: Math.ceil,
  round: Math.round,
  sqrt: (x) => Math.sqrt(Math.max(0, x)),
  pow: Math.pow,
  clamp: (x, lo, hi) => Math.min(hi, Math.max(lo, x)),
  if: (c, a, b) => (c ? a : b),
  contains: (s, sub) => String(s ?? '').toLowerCase().includes(String(sub ?? '').toLowerCase()),
  lower: (s) => String(s ?? '').toLowerCase(),
};

const KEYWORDS = { true: true, false: false, null: null, and: '&&', or: '||', not: '!' };

function tokenize(src) {
  const out = [];
  let i = 0;
  const n = src.length;
  while (i < n) {
    const c = src[i];
    if (c === ' ' || c === '\t' || c === '\n' || c === '\r') { i++; continue; }
    if ((c >= '0' && c <= '9') || (c === '.' && src[i + 1] >= '0' && src[i + 1] <= '9')) {
      let j = i;
      while (j < n && ((src[j] >= '0' && src[j] <= '9') || src[j] === '.')) j++;
      let v = parseFloat(src.slice(i, j));
      if (src[j] === '%') { v /= 100; j++; } // "15%" reads as 0.15
      out.push({ t: 'num', v });
      i = j;
      continue;
    }
    if (c === '"' || c === "'") {
      let j = i + 1;
      let s = '';
      while (j < n && src[j] !== c) { if (src[j] === '\\') j++; s += src[j] ?? ''; j++; }
      if (j >= n) throw new Error(`unterminated string in "${src}"`);
      out.push({ t: 'str', v: s });
      i = j + 1;
      continue;
    }
    if (/[A-Za-z_]/.test(c)) {
      let j = i;
      while (j < n && /[A-Za-z0-9_]/.test(src[j])) j++;
      const word = src.slice(i, j);
      const lw = word.toLowerCase();
      if (lw === 'in') out.push({ t: 'op', v: 'in' });
      else if (lw in KEYWORDS) {
        const k = KEYWORDS[lw];
        if (k === '&&' || k === '||' || k === '!') out.push({ t: 'op', v: k });
        else out.push({ t: 'lit', v: k });
      } else out.push({ t: 'id', v: word });
      i = j;
      continue;
    }
    const two = src.slice(i, i + 2);
    const three = src.slice(i, i + 3);
    if (three === '===' || three === '!==') { out.push({ t: 'op', v: three.slice(0, 2) }); i += 3; continue; }
    if (['&&', '||', '==', '!=', '<=', '>='].includes(two)) { out.push({ t: 'op', v: two }); i += 2; continue; }
    if ('+-*/%<>!?:(),.[]'.includes(c)) { out.push({ t: 'op', v: c }); i++; continue; }
    throw new Error(`unexpected character "${c}" in "${src}"`);
  }
  return out;
}

const BIN = { '||': 1, '&&': 2, '==': 3, '!=': 3, '<': 4, '<=': 4, '>': 4, '>=': 4, in: 4, '+': 5, '-': 5, '*': 6, '/': 6, '%': 6 };

function parse(src) {
  const toks = tokenize(src);
  let p = 0;
  const peek = () => toks[p];
  const isOp = (v) => toks[p] && toks[p].t === 'op' && toks[p].v === v;
  const eat = (v) => { if (!isOp(v)) throw new Error(`expected "${v}" in "${src}"`); p++; };

  function primary() {
    const tk = toks[p++];
    if (!tk) throw new Error(`unexpected end of "${src}"`);
    if (tk.t === 'num' || tk.t === 'str' || tk.t === 'lit') return { k: 'lit', v: tk.v };
    if (tk.t === 'id') {
      if (isOp('(')) {
        p++;
        const args = [];
        if (!isOp(')')) { do { args.push(ternary()); } while (isOp(',') && ++p); }
        eat(')');
        const name = tk.v.toLowerCase();
        if (!(name in FUNCS)) throw new Error(`unknown function "${tk.v}"`);
        return { k: 'call', f: name, args };
      }
      const path = [tk.v];
      while (isOp('.')) {
        p++;
        const nx = toks[p++];
        if (!nx || nx.t !== 'id') throw new Error(`bad member access in "${src}"`);
        path.push(nx.v);
      }
      return { k: 'var', path };
    }
    if (tk.t === 'op') {
      if (tk.v === '(') { const e = ternary(); eat(')'); return e; }
      if (tk.v === '[') {
        const items = [];
        if (!isOp(']')) { do { items.push(ternary()); } while (isOp(',') && ++p); }
        eat(']');
        return { k: 'list', items };
      }
      if (tk.v === '!') return { k: 'not', e: unary() };
      if (tk.v === '-') return { k: 'neg', e: unary() };
      if (tk.v === '+') return unary();
    }
    throw new Error(`unexpected token "${tk.v}" in "${src}"`);
  }
  function unary() { return primary(); }
  function binary(minPrec) {
    let left = unary();
    for (;;) {
      const tk = peek();
      if (!tk || tk.t !== 'op' || !(tk.v in BIN) || BIN[tk.v] < minPrec) return left;
      p++;
      const right = binary(BIN[tk.v] + 1);
      left = { k: 'bin', op: tk.v, l: left, r: right };
    }
  }
  function ternary() {
    const c = binary(1);
    if (isOp('?')) {
      p++;
      const a = ternary();
      eat(':');
      const b = ternary();
      return { k: 'tern', c, a, b };
    }
    return c;
  }
  const ast = ternary();
  if (p < toks.length) throw new Error(`unexpected "${toks[p].v}" in "${src}"`);
  return ast;
}

const BLOCKED = new Set(['__proto__', 'constructor', 'prototype']);
const eq = (a, b) => (typeof a === 'string' && typeof b === 'string' ? a.toLowerCase() === b.toLowerCase() : a == b); // eslint-disable-line eqeqeq
const num = (x) => (typeof x === 'number' ? x : x === true ? 1 : x == null || x === false ? 0 : Number(x) || 0);

function ev(n, scope) {
  switch (n.k) {
    case 'lit': return n.v;
    case 'var': {
      let v = scope;
      for (const key of n.path) {
        if (v == null || typeof v !== 'object' || BLOCKED.has(key) || !Object.prototype.hasOwnProperty.call(v, key)) return null;
        v = v[key];
      }
      return v;
    }
    case 'list': return n.items.map((x) => ev(x, scope));
    case 'not': return !ev(n.e, scope);
    case 'neg': return -num(ev(n.e, scope));
    case 'tern': return ev(n.c, scope) ? ev(n.a, scope) : ev(n.b, scope);
    case 'call': return FUNCS[n.f](...n.args.map((x) => ev(x, scope)));
    case 'bin': {
      if (n.op === '&&') return ev(n.l, scope) && ev(n.r, scope);
      if (n.op === '||') return ev(n.l, scope) || ev(n.r, scope);
      const a = ev(n.l, scope);
      const b = ev(n.r, scope);
      switch (n.op) {
        case '==': return eq(a, b);
        case '!=': return !eq(a, b);
        case 'in': return Array.isArray(b) ? b.some((x) => eq(a, x)) : false;
        case '<': return num(a) < num(b);
        case '<=': return num(a) <= num(b);
        case '>': return num(a) > num(b);
        case '>=': return num(a) >= num(b);
        case '+': return typeof a === 'string' || typeof b === 'string' ? `${a ?? ''}${b ?? ''}` : num(a) + num(b);
        case '-': return num(a) - num(b);
        case '*': return num(a) * num(b);
        case '/': { const d = num(b); return d === 0 ? 0 : num(a) / d; }
        case '%': { const d = num(b); return d === 0 ? 0 : num(a) % d; }
      }
    }
  }
  return null;
}

function collectVars(n, out) {
  if (!n || typeof n !== 'object') return;
  if (n.k === 'var') out.add(n.path[0]);
  for (const key of ['l', 'r', 'e', 'c', 'a', 'b']) if (n[key]) collectVars(n[key], out);
  for (const key of ['args', 'items']) if (n[key]) n[key].forEach((x) => collectVars(x, out));
}

const cache = new Map();

/** Compile an expression string. Throws on syntax errors. */
export function compile(src) {
  const key = String(src);
  let c = cache.get(key);
  if (!c) {
    const ast = parse(key);
    const vars = new Set();
    collectVars(ast, vars);
    c = { ast, vars: [...vars] };
    cache.set(key, c);
  }
  return c;
}

/** Evaluate an expression; a broken expression evaluates to null rather than throwing mid-simulation. */
export function evaluate(src, scope) {
  if (src == null || src === '') return null;
  if (typeof src === 'number' || typeof src === 'boolean') return src;
  try {
    return ev(compile(src).ast, scope);
  } catch {
    return null;
  }
}
export const evalNum = (src, scope, dflt = 0) => {
  const v = evaluate(src, scope);
  return typeof v === 'number' && Number.isFinite(v) ? v : v === true ? 1 : dflt;
};
export const evalBool = (src, scope, dflt = true) => {
  if (src == null || src === '') return dflt;
  return !!evaluate(src, scope);
};

/** Check an expression against the variables a context offers. Returns an error string or null. */
export function checkExpr(src, allowed) {
  if (typeof src === 'number' || typeof src === 'boolean') return null;
  if (typeof src !== 'string') return 'expression must be a string';
  if (src.trim() === '') return null;
  try {
    const { vars } = compile(src);
    const unknown = vars.filter((v) => !allowed.includes(v));
    if (unknown.length) return `unknown variable(s) ${unknown.join(', ')} in "${src}"`;
    return null;
  } catch (e) {
    return e.message;
  }
}
