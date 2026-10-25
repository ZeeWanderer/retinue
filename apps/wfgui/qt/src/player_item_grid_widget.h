#pragma once

#include <QJsonArray>
#include <QListView>
#include <QStringList>

#include <utility>

class QTimer;
class QShowEvent;
class QContextMenuEvent;

class PlayerItemGridWidget final : public QListView {
  Q_OBJECT

public:
  enum class Kind { Foundry, Inventory, Mastery };

  explicit PlayerItemGridWidget(Kind kind, QWidget *parent = nullptr);
  void setModel(QAbstractItemModel *model) override;
  void requestAllQuotes(bool refresh = false);
  void refreshVisibleQuotes();

signals:
  void assetsNeeded(const QJsonArray &assets);
  void quotesNeeded(const QStringList &items, bool refresh);
  void marketItemRequested(const QString &item, const QString &side);
  void relicRewardRequested(const QString &reward);
  void foundryItemRequested(const QString &item);

protected:
  void contextMenuEvent(QContextMenuEvent *event) override;
  void resizeEvent(QResizeEvent *event) override;
  void showEvent(QShowEvent *event) override;

private:
  void updateGrid();
  void scheduleVisibleData();
  void requestVisibleData();
  void requestVisibleAssets();
  void requestVisibleQuotes(bool refresh = false);
  void requestQuotes(int first, int last, bool refresh);
  std::pair<int, int> visibleRows() const;
  void preserveScrollPosition();
  void restoreScrollPosition();
  void clearScrollAnchor();

  Kind kind_;
  QTimer *visibleDataTimer_;
  QString preservedAnchorId_;
  int preservedAnchorOffset_ = 0;
  int preservedScrollValue_ = 0;
  bool scrollAnchorPending_ = false;
  bool scrollRestoreScheduled_ = false;
};
