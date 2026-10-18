export async function composeComparison({ sources, font }) {
  if (sources.length !== 2) throw new Error("Comparison needs both renders");
  if (font) {
    const face = new FontFace("Preview", `url(${font})`);
    document.fonts.add(await face.load());
  }
  const panels = [];
  for (const source of sources) {
    const image = new Image();
    image.src = source.image;
    await image.decode();
    const canvas = document.createElement("canvas");
    canvas.width = image.width;
    canvas.height = image.height;
    const context = canvas.getContext("2d", { willReadFrequently: true });
    context.drawImage(image, 0, 0);
    const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
    let left = canvas.width, top = canvas.height, right = -1, bottom = -1;
    for (let y = 0; y < canvas.height; y++) {
      for (let x = 0; x < canvas.width; x++) {
        if (!data[(y * canvas.width + x) * 4 + 3]) continue;
        left = Math.min(left, x);
        top = Math.min(top, y);
        right = Math.max(right, x);
        bottom = Math.max(bottom, y);
      }
    }
    if (right < left) throw new Error(`${source.label}: render is empty`);
    panels.push({ image, label: source.label, file: source.file,
      screen: { width: canvas.width, height: canvas.height },
      crop: { x: left, y: top, width: right - left + 1, height: bottom - top + 1 } });
  }
  const padding = 16, gap = 24, labelHeight = 28;
  const widths = panels.map(panel => Math.max(160, panel.crop.width));
  const canvas = document.createElement("canvas");
  canvas.width = padding * 2 + widths.reduce((sum, width) => sum + width, 0) + gap * (panels.length - 1);
  canvas.height = padding * 2 + labelHeight + Math.max(...panels.map(panel => panel.crop.height));
  const context = canvas.getContext("2d");
  context.fillStyle = "#303030";
  context.fillRect(0, 0, canvas.width, canvas.height);
  context.font = "14px Preview, sans-serif";
  context.fillStyle = "#ffffff";
  context.textBaseline = "top";
  let x = padding;
  const report = panels.map((panel, index) => {
    const { image, crop, ...metadata } = panel;
    context.fillText(panel.label, x, padding, widths[index]);
    const destination = { x, y: padding + labelHeight, width: crop.width, height: crop.height };
    context.drawImage(image, crop.x, crop.y, crop.width, crop.height,
      destination.x, destination.y, destination.width, destination.height);
    x += widths[index] + gap;
    return { ...metadata, crop, destination };
  });
  return { png: canvas.toDataURL("image/png").split(",")[1],
    report: { width: canvas.width, height: canvas.height, resampled: false, panels: report } };
}
