// Checks the TextMate grammar the way VS Code applies single-line rules:
// at each position the earliest match wins, ties go to the first rule.
const test = require('node:test');
const assert = require('node:assert');
const grammar = require('../syntaxes/globw.tmLanguage.json');

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
    assert.strictEqual(scope(line, 'prezzo'), 'variable.other.globw');
    assert.strictEqual(scope(line, 'quantità'), 'variable.other.globw');
    assert.strictEqual(scope(line, 'as'), 'keyword.other.as.globw');
    assert.strictEqual(scope(line, 'float'), 'storage.type.primitive.globw');
    assert.strictEqual(scope('    items as list of string', 'list of'), 'storage.type.globw');
    assert.strictEqual(scope('    d as dict of string to int', 'dict of'), 'storage.type.globw');
    assert.strictEqual(scope('variables:', 'variables:'), 'keyword.other.declaration.globw');
});

test('functions: definition, parameters, calls', () => {
    const def = 'function area(base as float) as float';
    assert.strictEqual(scope(def, 'function'), 'storage.type.function.globw');
    assert.strictEqual(scope(def, 'area'), 'entity.name.function.globw');
    assert.strictEqual(scope(def, 'base'), 'variable.other.globw');
    const call = 'x = half(3) + y';
    assert.strictEqual(scope(call, 'x'), 'variable.other.globw');
    assert.strictEqual(scope(call, 'half'), 'entity.name.function.globw');
    assert.strictEqual(scope(call, 'y'), 'variable.other.globw');
    assert.strictEqual(scope('output("hi " + name)', 'output'), 'support.function.builtin.globw');
    assert.strictEqual(scope('rect(x, 1, 2, 3, rgb(1, 2, 3))', 'rect'), 'support.function.builtin.globw');
    assert.strictEqual(scope('if(key("A") and mouse_x() > 3)', 'mouse_x'), 'support.function.builtin.globw');
    assert.strictEqual(scope('triangle3d(0, 0, 0, 1, 0, 0, 0, 1, 0, c)', 'triangle3d'), 'support.function.builtin.globw');
    assert.strictEqual(scope('x = sin(a) + round(b)', 'sin'), 'support.function.builtin.globw');
    assert.strictEqual(scope('w = text_width("hi")', 'text_width'), 'support.function.builtin.globw');
    assert.strictEqual(scope('n = to_int(s) + sum(l)', 'to_int'), 'support.function.builtin.globw');
    assert.strictEqual(scope('w = read_lines("a.txt")', 'read_lines'), 'support.function.builtin.globw');
    assert.strictEqual(scope('n = get(d, "a", 0) + length(keys(d))', 'keys'), 'support.function.builtin.globw');
    assert.strictEqual(scope('if(has_key(d, k))', 'has_key'), 'support.function.builtin.globw');
    assert.strictEqual(scope('lazy import shapes as s', 'lazy'), 'keyword.control.globw');
    assert.strictEqual(scope('lazy import shapes as s', 'import'), 'keyword.control.globw');
    assert.strictEqual(scope('from shapes import area', 'from'), 'keyword.control.globw');
    assert.strictEqual(scope('for(x in items)', 'in'), 'keyword.control.globw');
    assert.strictEqual(scope('        break', 'break'), 'keyword.control.globw');
    assert.strictEqual(scope('        continue', 'continue'), 'keyword.control.globw');
    assert.strictEqual(scope('for(x in items)', 'items'), 'variable.other.globw');
    assert.strictEqual(scope('class Point', 'class'), 'keyword.control.globw');
    assert.strictEqual(scope('        end class', 'end class'), 'keyword.control.globw');
    assert.strictEqual(scope('p = new Point(1, 2)', 'new'), 'keyword.control.globw');
    assert.strictEqual(scope('self.x = 1', 'self'), 'variable.language.self.globw');
    assert.strictEqual(scope('class Dog is Animal', 'is'), 'keyword.control.globw');
    assert.strictEqual(scope('parent.new(x)', 'parent'), 'variable.language.self.globw');
    assert.strictEqual(scope('try', 'try'), 'keyword.control.globw');
    assert.strictEqual(scope('    catch problem', 'catch'), 'keyword.control.globw');
    assert.strictEqual(scope('    always', 'always'), 'keyword.control.globw');
    assert.strictEqual(scope('        fail "x"', 'fail'), 'keyword.control.globw');
    assert.strictEqual(scope('    end try', 'end try'), 'keyword.control.globw');
    assert.strictEqual(scope('x = s.area(1, 2)', 'area'), 'entity.name.function.globw');
});

test('assignment and comparison', () => {
    assert.strictEqual(scope('x = 1', '='), 'keyword.operator.assignment.globw');
    assert.strictEqual(scope('if(x == 1)', '=='), 'keyword.operator.comparison.globw');
    assert.strictEqual(scope('if(x <= 1)', '<='), 'keyword.operator.comparison.globw');
});

test('keywords, constants, strings and comments are not variables', () => {
    const line = 'if(ok == true and n > 3) \' check';
    assert.strictEqual(scope(line, 'if'), 'keyword.control.globw');
    assert.strictEqual(scope(line, 'true'), 'constant.language.boolean.globw');
    assert.strictEqual(scope(line, 'and'), 'keyword.operator.word.globw');
    assert.strictEqual(scope(line, "' check"), 'comment.line.quote.globw');
    assert.strictEqual(scope('s = "a b"', '"a b"'), 'string.quoted.double.globw');
    assert.strictEqual(scope('end if', 'end if'), 'keyword.control.globw');
    assert.notStrictEqual(scope('IF(x)', 'IF'), 'keyword.control.globw', 'uppercase words are not keywords');
});
