const test = require('node:test');
const assert = require('node:assert');
const { parseErrors } = require('../parse');

test('one error with column, caret length and hint', () => {
    const stderr = [
        'Error at line 3, column 1: variable "count" is not declared',
        '    count <- total + 1',
        '    ^^^^^',
        'Hint: add it to "variables:" at the top of the program',
        '',
    ].join('\n');
    assert.deepStrictEqual(parseErrors(stderr), [
        {
            file: null,
            line: 2,
            col: 0,
            len: 5,
            message: 'variable "count" is not declared',
            hint: 'add it to "variables:" at the top of the program',
        },
    ]);
});

test('several errors separated by blank lines, Windows line endings', () => {
    const stderr = [
        'Error at line 1, column 1: variable "a" is not declared',
        '    a <- 1',
        '    ^',
        '',
        'Error at line 2, column 6: cannot use + between an integer and a boolean',
        '    b <- 1 + TRUE',
        '         ^',
        '',
    ].join('\r\n');
    const errors = parseErrors(stderr);
    assert.strictEqual(errors.length, 2);
    assert.deepStrictEqual(errors[1], {
        file: null,
        line: 1,
        col: 5,
        len: 1,
        message: 'cannot use + between an integer and a boolean',
        hint: null,
    });
});

test('error without column covers the whole line', () => {
    const errors = parseErrors('Error at line 4: division by zero\n    output(1 / 0)\n');
    assert.deepStrictEqual(errors, [{ file: null, line: 3, col: null, len: null, message: 'division by zero', hint: null }]);
});

test('no errors', () => {
    assert.deepStrictEqual(parseErrors(''), []);
});

test('errors in an imported module name their file', () => {
    const stderr = ['Error in shapes.gw at line 4, column 2: division by zero', '    x', '     ^', ''].join('\n');
    const [e] = parseErrors(stderr);
    assert.deepStrictEqual([e.file, e.line, e.col, e.len, e.message], ['shapes.gw', 3, 1, 1, 'division by zero']);
});
