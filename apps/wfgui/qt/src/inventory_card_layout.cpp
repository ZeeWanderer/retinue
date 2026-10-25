#include "inventory_card_layout.h"

#include <algorithm>

#include "display_metrics.h"

namespace wfgui {

InventoryCardLayout InventoryCardLayout::calculate(const QRect &content,
                                                    bool isSet,
                                                    int componentCount,
                                                    qreal scale) {
  InventoryCardLayout layout;
  const int leftWidth = std::min(content.width(), scaled(90, scale));
  const int columnGap = scaled(5, scale);
  const int ducatHeight = scaled(24, scale);
  const int titleHeight = scaled(27, scale);
  const int actionHeight = scaled(35, scale);

  layout.ducats = {content.left(), content.bottom() - ducatHeight + 1,
                   leftWidth, ducatHeight};
  layout.image = {content.left(), content.top(), leftWidth,
                  std::max(0, content.height() - ducatHeight)};

  const int rightLeft = content.left() + leftWidth + columnGap;
  const QRect right(rightLeft, content.top(),
                    std::max(0, content.right() - rightLeft + 1),
                    content.height());
  layout.title = {right.left(), right.top(), right.width(), titleHeight};

  if (!isSet) {
    const int quoteGap = scaled(11, scale);
    const int quoteWidth = std::max(0, (right.width() - quoteGap) / 2);
    const int quoteTop = right.bottom() - actionHeight + 1;
    layout.status = {right.left(), layout.title.bottom() + 1, right.width(),
                     std::max(0, quoteTop - layout.title.bottom() - 3)};
    layout.sell = {right.left(), quoteTop, quoteWidth, actionHeight};
    layout.buy = {right.left() + quoteWidth + quoteGap, quoteTop, quoteWidth,
                  actionHeight};
    return layout;
  }

  const int bodyTop = layout.title.bottom() + scaled(2, scale);
  const int bodyHeight = std::max(0, right.bottom() - bodyTop + 1);
  const int actionWidth = std::min(scaled(114, scale), right.width() / 3);
  const int actionLeft = right.right() - actionWidth + 1;
  layout.status = {actionLeft, bodyTop, actionWidth,
                   std::max(0, bodyHeight - actionHeight)};
  layout.sell = {actionLeft, right.bottom() - actionHeight + 1, actionWidth,
                 actionHeight};

  const QRect componentArea(
      right.left(), bodyTop,
      std::max(0, actionLeft - scaled(6, scale) - right.left()), bodyHeight);
  const int count = std::min(6, componentCount);
  const int cell = scaled(35, scale);
  const int gap = scaled(6, scale);
  const int columns = std::min(3, count);
  const int rows = columns == 0 ? 0 : (count + columns - 1) / columns;
  const int totalHeight = rows * cell + std::max(0, rows - 1) * gap;
  const int top = componentArea.center().y() - totalHeight / 2;
  layout.components.reserve(count);
  for (int row = 0, index = 0; index < count; ++row) {
    const int rowCount = std::min(columns, count - index);
    const int rowWidth = rowCount * cell + std::max(0, rowCount - 1) * gap;
    const int left = componentArea.center().x() - rowWidth / 2;
    for (int column = 0; column < rowCount; ++column, ++index) {
      layout.components.append(
          {left + column * (cell + gap), top + row * (cell + gap), cell, cell});
    }
  }
  return layout;
}

} // namespace wfgui
