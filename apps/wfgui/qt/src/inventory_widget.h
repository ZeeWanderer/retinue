#pragma once

#include <QWidget>

class AppController;
class AnimatedProgressBar;
class QLabel;
class PlayerItemFilterModel;
class PlayerItemGridWidget;
class QPushButton;
class QStackedLayout;
class QTimer;

class InventoryWidget final : public QWidget {
  Q_OBJECT

public:
  explicit InventoryWidget(AppController *controller,
                           QWidget *parent = nullptr);

signals:
  void marketItemRequested(const QString &item, const QString &side);
  void relicRewardRequested(const QString &reward);

private:
  bool priceSort() const;
  void beginPriceLoad(bool refresh = false);
  void updatePriceLoad();
  void updateContent();

  AppController *controller_;
  PlayerItemFilterModel *items_;
  PlayerItemGridWidget *grid_;
  QLabel *emptyState_;
  AnimatedProgressBar *progress_;
  QPushButton *refresh_;
  QStackedLayout *content_;
  QTimer *priceUpdateTimer_;
  QString sortMode_ = "name";
  int priceTotal_ = 0;
  int pendingPrices_ = 0;
  bool priceLoading_ = false;
};
