import { writeFile } from "node:fs/promises";

export async function saveWindowHtml(page, filename) {
  const html = await page.evaluate(() => {
    const root = document.documentElement.cloneNode(true);
    const base = document.createElement("base");
    base.href = document.baseURI;
    root.querySelectorAll("base").forEach(element => element.remove());
    root.querySelector("head").prepend(base);
    const styles = root.querySelectorAll("style");
    document.querySelectorAll("style").forEach((source, index) => {
      try {
        styles[index].textContent = [...source.sheet.cssRules].map(rule => rule.cssText).join("\n");
      } catch { /* Keep source text when CSSOM access is unavailable. */ }
    });
    const controls = root.querySelectorAll("input,textarea,select");
    document.querySelectorAll("input,textarea,select").forEach((source, index) => {
      const clone = controls[index];
      if (source.tagName === "INPUT") {
        clone.setAttribute("value", source.value);
        clone.toggleAttribute("checked", source.checked);
      } else if (source.tagName === "TEXTAREA") {
        clone.textContent = source.value;
      } else {
        [...clone.options].forEach((option, i) => option.toggleAttribute("selected", source.options[i].selected));
      }
    });
    root.querySelectorAll("script").forEach(script => script.remove());
    return `<!doctype html>\n${root.outerHTML}\n`;
  });
  await writeFile(filename, html);
}
