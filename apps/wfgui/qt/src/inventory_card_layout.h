#pragma once

#include <QList>
#include <QRect>

namespace wfgui {

struct InventoryCardLayout {
  QRect image;
  QRect ducats;
  QRect title;
  QRect status;
  QRect sell;
  QRect buy;
  QList<QRect> components;

  static InventoryCardLayout calculate(const QRect &content, bool isSet,
                                       int componentCount,
                                       qreal scale = 1.0);
};

} // namespace wfgui
