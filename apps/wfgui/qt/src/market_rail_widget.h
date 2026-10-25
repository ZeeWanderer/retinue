#pragma once

#include <QJsonArray>
#include <QWidget>

class AppController;
class MarketItemView;
class QCompleter;
class QEvent;
class QLabel;
class QLineEdit;
class QModelIndex;
class QStandardItemModel;
class QTimer;

class MarketRailWidget final : public QWidget {
  Q_OBJECT

public:
  explicit MarketRailWidget(AppController *controller,
                            QWidget *parent = nullptr);
  void showItem(const QString &item, const QString &side = "sell");

signals:
  void signInRequested();

private:
  bool eventFilter(QObject *watched, QEvent *event) override;
  void activateSuggestion(const QModelIndex &index);
  void showResolvedItem(const QString &item, const QString &label,
                        const QString &side);
  void requestSuggestions();
  void updateSuggestions(const QString &query, const QJsonArray &matches);

  AppController *controller_;
  QLineEdit *search_;
  QCompleter *completer_;
  QStandardItemModel *suggestions_;
  QLabel *empty_;
  MarketItemView *itemView_;
  QTimer *searchTimer_;
  QString latestQuery_;
  bool acceptingSuggestions_ = false;
};
