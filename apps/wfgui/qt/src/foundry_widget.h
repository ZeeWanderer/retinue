#pragma once

#include <QWidget>

class AppController;
class AnimatedProgressBar;
class CompactSearch;
class QButtonGroup;
class QLabel;
class PlayerItemFilterModel;
class PlayerItemGridWidget;
class QPushButton;
class QStackedLayout;

class FoundryWidget final : public QWidget {
  Q_OBJECT

public:
  explicit FoundryWidget(AppController *controller, QWidget *parent = nullptr);
  void showItem(const QString &item);

signals:
  void marketItemRequested(const QString &item, const QString &side);
  void relicRewardRequested(const QString &reward);

private:
  void updateContent();

  AppController *controller_;
  PlayerItemFilterModel *items_;
  PlayerItemGridWidget *grid_;
  QButtonGroup *groups_;
  CompactSearch *search_;
  QLabel *summary_;
  QLabel *emptyState_;
  AnimatedProgressBar *progress_;
  QPushButton *refresh_;
  QStackedLayout *content_;
};
