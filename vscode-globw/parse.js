// Reads the errors printed by `globw --check` into plain objects.
// Lines and columns are 0-based, ready for a VS Code Range.

const HEADER = /^Error at line (\d+)(?:, column (\d+))?: (.*)$/;
const CARET = /^\s*(\^+)\s*$/;
const HINT = /^Hint: (.*)$/;

function parseErrors(stderr) {
    const errors = [];
    let current = null;
    for (const raw of stderr.split('\n')) {
        const line = raw.replace(/\r$/, '');
        const header = HEADER.exec(line);
        if (header) {
            current = {
                line: Number(header[1]) - 1,
                col: header[2] ? Number(header[2]) - 1 : null,
                len: null,
                message: header[3],
                hint: null,
            };
            errors.push(current);
            continue;
        }
        if (!current) continue;
        const caret = CARET.exec(line);
        if (caret && current.col !== null && current.len === null) {
            current.len = caret[1].length;
            continue;
        }
        const hint = HINT.exec(line);
        if (hint) current.hint = hint[1];
    }
    return errors;
}

module.exports = { parseErrors };
