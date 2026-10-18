import assert from "node:assert/strict";

export default async function exercise(page) {
  const record = async label => {
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(resolve)));
    return page.evaluate(label => {
      const elements = [...document.querySelectorAll("button, .MuiSkeleton-root, [role=progressbar]")];
      return {label, elements: elements.map(element => {
        const style = getComputedStyle(element), after = getComputedStyle(element, "::after");
        return {text: element.innerText, className: element.className,
          animation: style.animation, transition: style.transition,
          after: {left: after.left, right: after.right, transition: after.transition},
          bounds: element.getBoundingClientRect().toJSON()};
      }), animations: document.getAnimations().map(animation => ({
        timing: animation.effect.getTiming(), frames: animation.effect.getKeyframes(),
      }))};
    }, label);
  };
  const evidence = [await record("ready")];
  await page.getByRole("button", {name: "Search", exact: true}).hover();
  evidence.push(await record("search hover"));
  await page.evaluate(() => {
    const {store, require, actions} = window.reference;
    const request = actions.findLast(action => action.meta?.trigger === "exile-prediction/predict-prices");
    store.dispatch({type: "exile-prediction/predict-prices/pending",
      meta: {arg: request.payload[0], requestId: require(56901).X(request.payload[0])}});
  });
  evidence.push(await record("prediction pending"));
  assert.equal(evidence.at(-1).elements.filter(e => e.className.includes("MuiSkeleton-root")).length, 3);
  await page.getByRole("button", {name: "Search", exact: true}).click();
  evidence.push(await record("search pending"));
  assert.ok(evidence.at(-1).elements.some(e => e.animation.includes("loadingAnimation")));
  return evidence;
}
