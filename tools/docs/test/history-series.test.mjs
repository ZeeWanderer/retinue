import assert from 'node:assert/strict';
import test from 'node:test';
import {rowsFor, timeline, trendsFor} from '../src/history-series.mjs';

test('interleaved configurations retain sample identity and missing metrics stay missing', () => {
  const part = (series, dates, values, ranges = {}) => ({
    config: {trend: 'setup', series, compiler: series, count: dates.length, last: dates.at(-1)},
    data: {points: dates.map(date => ({date, id: date})), values, spreads: ranges},
  });
  const parts = [part('new', ['2026-11-02'], {full: {p95: {0: 3}}, added: {p95: {0: 4}}}),
    part('old', ['2026-11-01', '2026-11-03'], {full: {p95: {0: 1, 1: 2}}},
      {full: {p95: {1: [1.5, 2.5]}}})];
  const [trend] = trendsFor(parts.map(p => p.config));
  assert.deepEqual([trend.count, trend.last], [3, '2026-11-03']);
  const {points, metrics} = timeline(parts);
  assert.deepEqual([...metrics.get('added')], ['p95']);
  const rows = rowsFor(points, 'full', 'p95');
  assert.deepEqual(rows.map(r => [r.value, r.change]), [[1, undefined], [3, 'Rust'], [2, 'Rust']]);
  assert.deepEqual(rows[2].spread, [1.5, 2.5]);
  assert.deepEqual(rowsFor(points, 'added', 'p95').map(r => r.value), [undefined, 4, undefined]);
});
