#pragma once

#include <QString>
#include <QWidget>

class AppController;
class AnimatedProgressBar;
class QLabel;
class MasterySummaryPanel;
class PlayerItemFilterModel;
class PlayerItemGridWidget;
class QProgressBar;
class QPushButton;
class QStackedLayout;
class QTimer;
class ThumbnailWidget;

class MasteryPlannerWidget final : public QWidget {
  Q_OBJECT

public:
  explicit MasteryPlannerWidget(AppController *controller,
                                QWidget *parent = nullptr);

signals:
  void marketItemRequested(const QString &item, const QString &side);
  void relicRewardRequested(const QString &reward);
  void foundryItemRequested(const QString &item);

private:
  void setMode(const QString &mode);
  void beginPriceLoad();
  void updatePriceLoad();
  void updateContent();

  AppController *controller_;
  PlayerItemFilterModel *items_;
  PlayerItemGridWidget *grid_;
  QLabel *rank_;
  ThumbnailWidget *rankIcon_;
  QLabel *completionPercent_;
  QLabel *completionText_;
  MasterySummaryPanel *gameContent_;
  MasterySummaryPanel *starChart_;
  MasterySummaryPanel *intrinsics_;
  QLabel *emptyState_;
  QProgressBar *completionBar_;
  AnimatedProgressBar *loadingBar_;
  QPushButton *refresh_;
  QStackedLayout *content_;
  QTimer *priceUpdateTimer_;
  QString mode_ = "easy";
  bool priceLoading_ = false;
};
