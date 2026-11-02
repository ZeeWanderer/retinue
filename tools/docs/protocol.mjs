import fs from 'node:fs/promises';
import path from 'node:path';
import protobuf from 'protobufjs';

export const schemas = [
  'apps/retinue_core/proto/retinue_wire.proto',
  'apps/wfcore/proto/wf_native.proto',
  'apps/wfcore/proto/wf_views.proto',
  'apps/poe2core/proto/poe2_native.proto',
];

const cell = value => String(value ?? '').replaceAll('|', '\\|').replaceAll('\n', '<br>');
const anchor = name => name.replace(/^\./, '').toLowerCase().replaceAll('.', '');

export async function protocolPages(root, link) {
  const model = new protobuf.Root();
  const owners = new Map();
  for (const source of schemas) {
    const before = new Set(types(model));
    protobuf.parse(await fs.readFile(path.join(root, source), 'utf8'), model, {keepCase: true, alternateCommentMode: true});
    for (const item of types(model)) if (!before.has(item)) owners.set(item, source);
  }
  model.resolveAll();
  const route = source => `reference/protocol/${path.basename(source, '.proto')}`;
  const reference = type => type && owners.has(type)
    ? `[${type.fullName.slice(1)}](${link(route(owners.get(type)) + '/#' + anchor(type.fullName))})`
    : undefined;
  return schemas.map(source => {
    const items = [...owners].filter(([, owner]) => owner === source).map(([item]) => item);
    const blocks = items.map(item => {
      const title = `## ${item.fullName.slice(1)}\n\n${item.comment ?? ''}\n`;
      if (item instanceof protobuf.Enum) {
        return title + '\n| Value | Number | Description |\n| --- | --- | --- |\n' +
          Object.entries(item.values).map(([name, number]) => `| \`${name}\` | ${number} | ${cell(item.comments?.[name])} |`).join('\n');
      }
      const fields = item.fieldsArray.map(field => {
        const presence = field.map ? `map<${field.keyType}>` : field.repeated ? 'repeated' : field.options?.proto3_optional ? 'optional' : field.partOf ? `oneof ${field.partOf.name}` : 'singular';
        return `| \`${field.name}\` | ${field.id} | ${reference(field.resolvedType) ?? `\`${field.type}\``} | ${cell(presence)} | ${cell(field.comment)} |`;
      });
      return title + '\n| Field | Tag | Type | Presence | Description |\n| --- | --- | --- | --- | --- |\n' + fields.join('\n');
    });
    return {route: route(source), title: path.basename(source), source,
      body: `Generated from the owning schema. [Source](SOURCE_URL). Generated language bindings are not a second schema authority.\n\n${blocks.join('\n\n')}`};
  });
}

function types(namespace) {
  return Object.values(namespace.nested ?? {}).flatMap(item => [
    ...((item instanceof protobuf.Type || item instanceof protobuf.Enum) ? [item] : []), ...types(item),
  ]);
}
