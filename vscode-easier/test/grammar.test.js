// Checks the TextMate grammar the way VS Code applies single-line rules:
// at each position the earliest match wins, ties go to the first rule.
const test = require('node:test');
const assert = require('node:assert');
const grammar = require('../syntaxes/easier.tmLanguage.json');

function rules() {
    const out = [];
    const add = (r) => {
        if (r.include) return add(grammar.repository[r.include.slice(1)]);
        if (r.begin) {
            // strings: begin/end treated as one match for the purpose of this test
            out.push({ re: new RegExp('"(?:[^"\\\\]|\\\\.)*"?', 'gu'), name: r.name });
            return;
        }
        if (r.patterns) return r.patterns.forEach(add);
        out.push({ re: new RegExp(r.match, 'gu'), name: r.name, captures: r.captures });
    };
    grammar.patterns.forEach(add);
    return out;
}

/** Returns [text, scope] pairs for the scoped tokens of one line. */
function tokenize(line) {
    const all = rules();
    const tokens = [];
    let pos = 0;
    while (pos < line.length) {
        let best = null;
        for (const r of all) {
            r.re.lastIndex = pos;
            const m = r.re.exec(line);
            if (m && m[0].length > 0 && (!best || m.index < best.m.index)) best = { r, m };
        }
        if (!best) break;
        const { r, m } = best;
        if (r.captures) {
            for (const [k, c] of Object.entries(r.captures)) if (m[k]) tokens.push([m[k], c.name]);
        } else {
            tokens.push([m[0], r.name]);
        }
        pos = m.index + m[0].length;
    }
    return tokens;
}

const scope = (line, text) => (tokenize(line).find(([t]) => t === text) || [])[1];

test('every rule compiles', () => {
    assert.ok(rules().length > 10);
});

test('declarations: variables, as, types', () => {
    const line = '    prezzo, quantità as float';
    assert.strictEqual(scope(line, 'prezzo'), 'variable.other.easier');
    assert.strictEqual(scope(line, 'quantità'), 'variable.other.easier');
    assert.strictEqual(scope(line, 'as'), 'keyword.other.as.easier');
    assert.strictEqual(scope(line, 'float'), 'storage.type.primitive.easier');
    assert.strictEqual(scope('    items as list of string', 'list of'), 'storage.type.easier');
    assert.strictEqual(scope('variables:', 'variables:'), 'keyword.other.declaration.easier');
});

test('functions: definition, parameters, calls', () => {
    const def = 'FUNCTION area(base as float) as float';
    assert.strictEqual(scope(def, 'FUNCTION'), 'storage.type.function.easier');
    assert.strictEqual(scope(def, 'area'), 'entity.name.function.easier');
    assert.strictEqual(scope(def, 'base'), 'variable.other.easier');
    const call = 'x <- half(3) + y';
    assert.strictEqual(scope(call, 'x'), 'variable.other.easier');
    assert.strictEqual(scope(call, 'half'), 'entity.name.function.easier');
    assert.strictEqual(scope(call, 'y'), 'variable.other.easier');
    assert.strictEqual(scope('output("hi " + name)', 'output'), 'support.function.builtin.easier');
    assert.strictEqual(scope('RECT(x, 1, 2, 3, RGB(1, 2, 3))', 'RECT'), 'support.function.builtin.easier');
    assert.strictEqual(scope('IF(KEY("A") AND MOUSE_X() > 3)', 'MOUSE_X'), 'support.function.builtin.easier');
});

test('keywords, constants, strings and comments are not variables', () => {
    const line = 'IF(ok = TRUE AND n > 3) \' check';
    assert.strictEqual(scope(line, 'IF'), 'keyword.control.easier');
    assert.strictEqual(scope(line, 'TRUE'), 'constant.language.boolean.easier');
    assert.strictEqual(scope(line, 'AND'), 'keyword.operator.word.easier');
    assert.strictEqual(scope(line, "' check"), 'comment.line.quote.easier');
    assert.strictEqual(scope('s <- "a b"', '"a b"'), 'string.quoted.double.easier');
    assert.strictEqual(scope('END IF', 'END IF'), 'keyword.control.easier');
});
