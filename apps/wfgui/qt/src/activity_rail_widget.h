#pragma once

#include <QJsonObject>
#include <QWidget>

class AppController;
class QLabel;
class QGridLayout;
class QPropertyAnimation;
class QPushButton;
class QStackedWidget;
class QVBoxLayout;
class MarketRailWidget;

class ActivityRailWidget final : public QWidget {
  Q_OBJECT

public:
  explicit ActivityRailWidget(AppController *controller,
                              QWidget *parent = nullptr);
  bool setTab(const QString &tab);
  void showMarketItem(const QString &item, const QString &side = "sell");

signals:
  void signInRequested();
  void relicEraRequested(const QString &era);

protected:
  bool eventFilter(QObject *watched, QEvent *event) override;

private:
  void rebuild();
  void rebuildCycles(const QJsonObject &data);
  void rebuildFissures(const QJsonObject &data);
  void setStatus(const QString &error);
  void updateCountdowns();

  AppController *controller_;
  QGridLayout *cycles_;
  QVBoxLayout *fissures_;
  QLabel *resurgence_;
  QLabel *baro_;
  QLabel *status_;
  QPushButton *timersTab_;
  QPushButton *marketTab_;
  QStackedWidget *pages_;
  MarketRailWidget *market_;
  QPropertyAnimation *statusAnimation_;
  QString mode_ = "all";
};
