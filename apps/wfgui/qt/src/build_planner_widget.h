#pragma once

#include <QWidget>

class AnimatedProgressBar;
class AppController;
class BuildDiscoverWidget;
class BuildEquipmentWidget;
class BuildGroupsWidget;
class QLabel;
class OverframeLogin;
class QPushButton;
class QShowEvent;
class QStackedWidget;

class BuildPlannerWidget final : public QWidget {
  Q_OBJECT

public:
  explicit BuildPlannerWidget(AppController *controller,
                              QWidget *parent = nullptr);
  bool setMode(const QString &mode);

protected:
  void showEvent(QShowEvent *event) override;

private:
  void updateState();
  void showGroups(const QString &groupId = QString());
  void showEquipment(const QString &definitionId = QString(),
                     const QString &instanceId = QString());
  void showDiscover(const QString &definitionId = QString());

  AppController *controller_;
  OverframeLogin *login_;
  QLabel *accountName_;
  QLabel *error_;
  QPushButton *signIn_;
  QPushButton *signOut_;
  QPushButton *groupsMode_;
  QPushButton *equipmentMode_;
  QPushButton *discoverMode_;
  AnimatedProgressBar *progress_;
  QStackedWidget *modes_;
  BuildGroupsWidget *groups_;
  BuildEquipmentWidget *equipment_;
  BuildDiscoverWidget *discover_;
  QString loginError_;
};
