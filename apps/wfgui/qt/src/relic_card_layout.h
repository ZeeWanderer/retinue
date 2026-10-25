#pragma once

#include <QList>
#include <QRect>

namespace wfgui {

struct RelicCardLayout {
  struct RefinementRow {
    QRect label;
    QRect platinum;
    QRect ducats;
  };

  QRect card;
  QRect image;
  QRect relicArt;
  QRect amountBadge;
  QRect vaultedBadge;
  QRect middle;
  QRect title;
  QRect platinumHeader;
  QRect ducatHeader;
  QRect rewards;
  QList<RefinementRow> refinementRows;
  QList<QRect> rewardCells;

  static RelicCardLayout calculate(const QRect &bounds, int refinementCount,
                                   int rewardCount, qreal scale = 1.0);
};

int relicGridColumns(int viewportWidth, qreal scale = 1.0);

} // namespace wfgui
