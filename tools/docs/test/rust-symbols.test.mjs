import {test} from 'node:test';
import assert from 'node:assert/strict';
import {declaration, rustPages} from '../rust-symbols.mjs';

test('source declarations preserve nested bounds but never publish function bodies', () => {
  const src = 'pub fn make<T: Fn() -> usize>(x:T) -> usize where T: Send { x() }';
  assert.equal(declaration(src, 'function'), src.split(' {')[0]);
  assert.equal(declaration('pub trait View: Send { fn draw(&self); }', 'trait'), 'pub trait View: Send');
});

test('different specializations of the same trait have distinct, order-independent method links', () => {
  const make = (order) => {
    const index = {
      0: {name: 'kit', inner: {module: {items: [1]}}},
      1: {name: 'X', inner: {struct: {impls: order}}},
    };
    for (const n of [2, 3]) {
      index[n] = {
        inner: {
          impl: {
            trait: {path: 'Convert', id: 8},
            for: {primitive: n === 2 ? 'u8' : 'u16'},
            items: [n + 2],
          },
        },
      };
      index[n + 2] = {
        name: 'convert',
        inner: {function: {has_body: true}},
        docs: n === 2 ? 'small' : 'large',
      };
    }
    const paths = {
      0: {path: ['kit'], kind: 'module', crate_id: 0},
      1: {path: ['kit', 'X'], kind: 'struct', crate_id: 0},
    };
    const x = rustPages(
      {format_version: 61, root: 0, index, paths},
      new Set(),
      () => undefined,
      new Set(['kit']),
    ).find((p) => p.title === 'X');
    return Object.fromEntries(
      x.groups
        .flatMap((g) => g.items)
        .filter((r) => r.anchor)
        .map((r) => [r.description, r.anchor]),
    );
  };
  const anchors = make([2, 3]);
  assert.notEqual(anchors.small, anchors.large);
  assert.deepEqual(anchors, make([3, 2]));
});
