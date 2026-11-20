export function trendsFor(configs) {
  const trends = new Map();
  for (const config of configs) {
    const trend = trends.get(config.trend) ?? {...config, series: config.trend, members: [], count: 0};
    trend.members.push(config);
    trend.count += config.count;
    if (config.last > trend.last) trend.last = config.last;
    trends.set(config.trend, trend);
  }
  return [...trends.values()].sort((a, b) => b.last.localeCompare(a.last));
}

export function timeline(parts) {
  const points = [], metrics = new Map();
  for (const {config, data} of parts) {
    data.points.forEach((point, index) => points.push({...point, index, config, data}));
    for (const [workload, columns] of Object.entries(data.values)) {
      const names = metrics.get(workload) ?? new Set();
      for (const name of Object.keys(columns)) names.add(name);
      metrics.set(workload, names);
    }
  }
  points.sort((a, b) => Date.parse(a.date) - Date.parse(b.date) || a.id.localeCompare(b.id));
  return {points, metrics};
}

export function rowsFor(points, workload, metric) {
  let previous;
  return points.map(({index, config, data, ...p}, i) => {
    const value = data.values[workload]?.[metric]?.[index];
    let change;
    if (value != null && previous && previous.series !== config.series) {
      const fields = {compiler: 'Rust', nativeCompiler: 'native compiler', kernel: 'kernel',
        affinity: 'CPU affinity', features: 'build features', fixture: 'fixture'};
      change = Object.entries(fields)
        .filter(([key]) => JSON.stringify(previous[key]) !== JSON.stringify(config[key]))
        .map(([, label]) => label).join(', ') || 'other recorded settings';
    }
    if (value != null) previous = config;
    return {...p, config, change, ordinal: i + 1, at: new Date(p.date), value,
      spread: data.spreads?.[workload]?.[metric]?.[index]};
  });
}
