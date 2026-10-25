#include "relic_card_layout.h"

#include <algorithm>

#include "display_metrics.h"

namespace {
constexpr int ItemInset = 4;
constexpr int ContentPadding = 6;
constexpr int ColumnGap = 10;
constexpr int ImageWidth = 80;
constexpr int RewardsWidth = 127;
constexpr int MaxMiddleWidth = 220;
constexpr int HeaderHeight = 23;
constexpr int RefinementHeight = 18;
constexpr int CompactRefinementHeight = 16;
constexpr int RewardSize = 37;
constexpr int RewardGap = 4;
constexpr int RewardColumns = 3;
constexpr int MaxRefinements = 4;
constexpr int MaxRewards = 6;
constexpr int ValueColumnWidth = 34;
constexpr int MinimumTrackWidth = 420;
constexpr int TargetTrackWidth = 520;
} // namespace

namespace wfgui {

RelicCardLayout RelicCardLayout::calculate(const QRect &bounds,
                                           int refinementCount, int rewardCount,
                                           qreal scale) {
  RelicCardLayout result;
  const int itemInset = wfgui::scaled(ItemInset, scale);
  const int contentPadding = wfgui::scaled(ContentPadding, scale);
  const int columnGap = wfgui::scaled(ColumnGap, scale);
  const int imageWidth = wfgui::scaled(ImageWidth, scale);
  const int rewardsWidth = wfgui::scaled(RewardsWidth, scale);
  const int maxMiddleWidth = wfgui::scaled(MaxMiddleWidth, scale);
  const int headerHeight = wfgui::scaled(HeaderHeight, scale);
  const int refinementHeight = wfgui::scaled(RefinementHeight, scale);
  const int compactRefinementHeight =
      wfgui::scaled(CompactRefinementHeight, scale);
  const int rewardSize = wfgui::scaled(RewardSize, scale);
  const int rewardGap = wfgui::scaled(RewardGap, scale);
  const int valueColumnWidth = wfgui::scaled(ValueColumnWidth, scale);

  result.card = bounds.adjusted(itemInset, itemInset, -itemInset, -itemInset);
  const QRect content = result.card.adjusted(contentPadding, contentPadding,
                                             -contentPadding, -contentPadding);

  const int availableMiddle =
      std::max(0, content.width() - imageWidth - rewardsWidth - 2 * columnGap);
  const int middleWidth = std::min(maxMiddleWidth, availableMiddle);
  const int groupWidth =
      imageWidth + middleWidth + rewardsWidth + 2 * columnGap;
  const int groupLeft = content.center().x() - groupWidth / 2;
  result.image = {groupLeft, content.top(), imageWidth, content.height()};
  result.relicArt =
      result.image.adjusted(wfgui::scaled(4, scale), wfgui::scaled(2, scale),
                            -wfgui::scaled(4, scale), -wfgui::scaled(2, scale));
  result.amountBadge = {result.image.left() + wfgui::scaled(18, scale),
                        result.image.bottom() - wfgui::scaled(19, scale),
                        wfgui::scaled(44, scale), wfgui::scaled(20, scale)};
  result.vaultedBadge = {result.image.left() + wfgui::scaled(2, scale),
                         result.image.top() + wfgui::scaled(2, scale),
                         wfgui::scaled(23, scale), wfgui::scaled(23, scale)};

  const int middleLeft = result.image.right() + columnGap + 1;
  result.middle = {middleLeft, content.top(), middleWidth, content.height()};
  result.rewards = {result.middle.right() + columnGap + 1, content.top(),
                    rewardsWidth, content.height()};
  const int rows = std::clamp(refinementCount, 1, MaxRefinements);
  const int rowHeight = rows >= 4 ? compactRefinementHeight : refinementHeight;
  const int groupHeight = headerHeight + rows * rowHeight;
  const int groupTop = result.middle.center().y() - groupHeight / 2;
  const int valuesLeft = result.middle.right() - 2 * valueColumnWidth + 1;
  result.title = {
      result.middle.left(), groupTop,
      std::max(0, valuesLeft - result.middle.left() - wfgui::scaled(6, scale)),
      headerHeight};
  result.platinumHeader = {valuesLeft, groupTop, valueColumnWidth,
                           headerHeight};
  result.ducatHeader = {valuesLeft + valueColumnWidth, groupTop,
                        valueColumnWidth, headerHeight};

  int top = groupTop + headerHeight;
  for (int row = 0; row < rows; ++row) {
    result.refinementRows.push_back({
        .label = {result.middle.left(), top,
                  std::max(0, valuesLeft - result.middle.left() -
                                  wfgui::scaled(6, scale)),
                  rowHeight},
        .platinum = {valuesLeft, top, valueColumnWidth, rowHeight},
        .ducats = {valuesLeft + valueColumnWidth, top, valueColumnWidth,
                   rowHeight},
    });
    top += rowHeight;
  }

  const int count = std::clamp(rewardCount, 0, MaxRewards);
  const int gridWidth =
      rewardSize * RewardColumns + rewardGap * (RewardColumns - 1);
  const int gridHeight = rewardSize * 2 + rewardGap;
  const int gridLeft = result.rewards.center().x() - gridWidth / 2;
  const int gridTop = result.rewards.center().y() - gridHeight / 2;
  for (int index = 0; index < count; ++index) {
    const int column = index % RewardColumns;
    const int row = index / RewardColumns;
    result.rewardCells.push_back({gridLeft + column * (rewardSize + rewardGap),
                                  gridTop + row * (rewardSize + rewardGap),
                                  rewardSize, rewardSize});
  }

  return result;
}

int relicGridColumns(int viewportWidth, qreal scale) {
  const int width = std::max(1, viewportWidth);
  const int targetTrackWidth = wfgui::scaled(TargetTrackWidth, scale);
  const int minimumTrackWidth = wfgui::scaled(MinimumTrackWidth, scale);
  const int targetColumns =
      std::max(1, (width + targetTrackWidth / 2) / targetTrackWidth);
  const int maximumColumns = std::max(1, width / minimumTrackWidth);
  return std::min(targetColumns, maximumColumns);
}

} // namespace wfgui
