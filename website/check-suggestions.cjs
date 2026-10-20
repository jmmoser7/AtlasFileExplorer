const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync('website/suggestions/suggestions.js', 'utf8');
function submit(title, body, kind = 'Suggestion') {
  let handler, destination, prevented = false, invalid = false;
  const fields = Object.fromEntries(Object.entries({title, body, kind}).map(([key, value]) =>
    ['#' + key, {value, setCustomValidity() {}, reportValidity() { invalid = true; }, addEventListener() {}}]));
  const form = {action: 'https://github.com/jmmoser7/AtlasFileExplorer/issues/new',
    addEventListener(type, fn) { handler = fn; }};
  vm.runInNewContext(source, {URL, document: {querySelector(selector) {
    return selector === '.suggestion-form' ? form : fields[selector];
  }}, window: {location: {assign(url) { destination = new URL(url); }}}});
  handler({preventDefault() { prevented = true; }});
  return {destination, prevented, invalid};
}
const result = submit('  A & B #1  ', 'Line one\nUnicode: café / 画像 & ? #', 'Bug');
assert.equal(result.destination.origin, 'https://github.com');
assert.equal(result.destination.pathname, '/jmmoser7/AtlasFileExplorer/issues/new');
assert.equal(result.destination.searchParams.get('title'), '[Bug] A & B #1');
assert.match(result.destination.searchParams.get('body'), /Line one\nUnicode: café \/ 画像 & \? #/);
assert.equal(result.prevented, true);
const blank = submit('   ', 'Some detail');
assert.equal(blank.invalid, true);
assert.equal(blank.destination, undefined);
console.log('PASS: suggestion draft preserves Unicode and special characters; empty content stays on page.');
