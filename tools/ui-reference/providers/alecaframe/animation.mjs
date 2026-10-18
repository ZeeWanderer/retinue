export function suggestionScrollOffset(elapsed, maximum) {
  const time = ((elapsed % 4) + 4) % 4;
  if (time < 0.5) return 0;
  if (time < 1.5) return Math.round(maximum * smoothstep(time - 0.5));
  if (time < 2.25) return maximum;
  if (time < 3.25) return Math.round(maximum * (1 - smoothstep(time - 2.25)));
  return 0;
}

function smoothstep(value) {
  return value * value * (3 - 2 * value);
}
