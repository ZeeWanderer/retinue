#pragma once

#include <QHash>
#include <QJsonObject>
#include <QList>
#include <QWidget>

class AppController;
class AnimatedProgressBar;
class MarketOrderCard;
class QButtonGroup;
class QComboBox;
class QGridLayout;
class QHideEvent;
class QLabel;
class QLineEdit;
class QPushButton;
class QScrollArea;
class QStackedWidget;
class QShowEvent;
class QTimer;
class QToolButton;

class MarketWidget final : public QWidget {
  Q_OBJECT

public:
  explicit MarketWidget(AppController *controller, QWidget *parent = nullptr);

signals:
  void marketItemRequested(const QString &item, const QString &side);

protected:
  bool eventFilter(QObject *watched, QEvent *event) override;
  void showEvent(QShowEvent *event) override;
  void hideEvent(QHideEvent *event) override;

private:
  void updateState();
  void scheduleRebuild();
  void rebuildOrders();
  void relayoutOrders(bool force = false);
  void refreshQuotes(bool force);
  void setView(QWidget *view);
  void editOrder(const QJsonObject &order);
  void deleteFiltered();
  void reconcileSellOrders();
  QString orderName(const QJsonObject &order) const;
  QString orderCategory(const QJsonObject &order) const;
  bool orderMissing(const QJsonObject &order) const;

  AppController *controller_;
  QStackedWidget *views_;
  QWidget *loadingView_;
  QWidget *loginView_;
  QWidget *ordersView_;
  QLineEdit *email_;
  QLineEdit *password_;
  QLabel *loginError_;
  QPushButton *login_;
  QLabel *accountName_;
  QComboBox *presence_;
  QLabel *presenceState_;
  QLabel *summary_;
  AnimatedProgressBar *progress_;
  QButtonGroup *categories_;
  QLineEdit *search_;
  QComboBox *side_;
  QComboBox *visibility_;
  QComboBox *inventory_;
  QComboBox *sort_;
  QToolButton *direction_;
  QScrollArea *scroll_;
  QWidget *orderHost_;
  QGridLayout *orderGrid_;
  QTimer *refreshTimer_;
  QTimer *rebuildTimer_;
  QList<QWidget *> orderCards_;
  QHash<QString, MarketOrderCard *> cardsById_;
  QHash<QString, QString> cardAssets_;
  QLabel *empty_ = nullptr;
  QList<QJsonObject> filteredOrders_;
  QString category_ = "all";
  bool descending_ = false;
  bool rebuildPending_ = false;
  int columns_ = 0;
};
