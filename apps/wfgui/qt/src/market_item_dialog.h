#pragma once

#include <QDialog>
#include <QJsonArray>
#include <QJsonObject>
#include <QList>
#include <QString>
#include <QWidget>

class AppController;
class AnimatedProgressBar;
class QLabel;
class QButtonGroup;
class QComboBox;
class QListWidget;
class QPushButton;
class QSpinBox;
class QTableWidget;
class QTimer;
class QWidget;
class ThumbnailWidget;

class MarketItemView final : public QWidget {
  Q_OBJECT

public:
  enum class Presentation { Dialog, Rail };

  explicit MarketItemView(AppController *controller, Presentation presentation,
                          QWidget *parent = nullptr);
  void showItem(const QString &item, const QString &side = QString());

signals:
  void signInRequested();
  void titleChanged(const QString &title);

private:
  void setMode(const QString &side);
  void updateContent();
  void updateListings(const QJsonArray &orders);
  void updatePostingFields(const QJsonObject &item);
  void requestData(bool refresh);
  void requestVariantData(bool refresh = false);
  void copyWhisper();
  void setCompactCopied(int row, bool copied);
  void postOrder();
  QJsonObject postingFilters() const;
  QString listingPlayer(const QJsonObject &order) const;

  AppController *controller_;
  ThumbnailWidget *image_;
  QLabel *name_;
  QLabel *status_;
  QButtonGroup *modes_;
  QTableWidget *listings_;
  QListWidget *compactListings_;
  AnimatedProgressBar *progress_;
  QSpinBox *quantity_;
  QSpinBox *price_;
  QSpinBox *rank_;
  QSpinBox *charges_;
  QSpinBox *amberStars_;
  QSpinBox *cyanStars_;
  QSpinBox *perTrade_;
  QComboBox *subtype_;
  QWidget *rankRow_;
  QWidget *chargesRow_;
  QWidget *amberStarsRow_;
  QWidget *cyanStarsRow_;
  QWidget *perTradeRow_;
  QWidget *subtypeRow_;
  QTimer *variantTimer_;
  QTimer *copyResetTimer_;
  QPushButton *copy_;
  QPushButton *post_;
  QPushButton *signIn_;
  QList<QJsonObject> listingRows_;
  QString itemKey_;
  QJsonObject variantQuote_;
  QJsonObject variantQuoteFilters_;
  Presentation presentation_;
  QString mode_ = "sell";
  QString displayedTitle_;
  int copiedRow_ = -1;
  bool posting_ = false;
  bool initializePrice_ = true;
};

class MarketItemDialog final : public QDialog {
  Q_OBJECT

public:
  explicit MarketItemDialog(AppController *controller,
                            QWidget *parent = nullptr);
  void showItem(const QString &item, const QString &side = QString());

signals:
  void signInRequested();

private:
  MarketItemView *view_;
};
