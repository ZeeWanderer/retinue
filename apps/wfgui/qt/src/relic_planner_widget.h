#pragma once

#include <QWidget>

class AppController;
class AnimatedProgressBar;
class CompactSearch;
class QButtonGroup;
class QLabel;
class QPushButton;
class QStackedLayout;
class RelicGridWidget;

class RelicPlannerWidget final : public QWidget {
  Q_OBJECT

public:
  explicit RelicPlannerWidget(AppController *controller,
                              QWidget *parent = nullptr);
  void showEra(const QString &era);
  void showReward(const QString &reward);

signals:
  void marketItemRequested(const QString &item, const QString &side);
  void foundryItemRequested(const QString &item);

private:
  void updateContent();
  void updateEra();
  void updateEraIcons();

  AppController *controller_;
  QLabel *traceCount_;
  AnimatedProgressBar *priceProgress_;
  QLabel *emptyState_;
  RelicGridWidget *relics_;
  CompactSearch *search_;
  QPushButton *refresh_;
  QButtonGroup *eraGroup_;
  QStackedLayout *content_;
};
