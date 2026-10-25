#include "market_rail_widget.h"

#include <QAbstractItemView>
#include <QCompleter>
#include <QEvent>
#include <QJsonArray>
#include <QJsonObject>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QSignalBlocker>
#include <QStandardItemModel>
#include <QTimer>
#include <QVBoxLayout>

#include "app_controller.h"
#include "market_item_dialog.h"
#include "widget_capture.h"

namespace {
constexpr int ItemKeyRole = Qt::UserRole + 1;
constexpr auto TypedTextProperty = "marketTypedText";

class MarketCompleter final : public QCompleter {
public:
  using QCompleter::QCompleter;

  QString pathFromIndex(const QModelIndex &index) const override {
    if (const QWidget *editor = widget()) {
      return editor->property(TypedTextProperty).toString();
    }
    return QCompleter::pathFromIndex(index);
  }
};
} // namespace

MarketRailWidget::MarketRailWidget(AppController *controller, QWidget *parent)
    : QWidget(parent), controller_(controller), search_(new QLineEdit),
      completer_(new MarketCompleter(this)),
      suggestions_(new QStandardItemModel(this)), empty_(new QLabel),
      itemView_(
          new MarketItemView(controller, MarketItemView::Presentation::Rail)),
      searchTimer_(new QTimer(this)) {
  setObjectName("marketRail");
  wfgui::setCaptureTarget(this, "right-rail.market");
  auto *layout = new QVBoxLayout(this);
  layout->setContentsMargins(8, 8, 8, 6);
  layout->setSpacing(8);

  search_->setObjectName("marketRailSearch");
  wfgui::setCaptureTarget(search_, "right-rail.market.search");
  search_->setPlaceholderText("WFMarket search");
  search_->setClearButtonEnabled(true);
  search_->setProperty(TypedTextProperty, QString());
  completer_->setModel(suggestions_);
  completer_->setCaseSensitivity(Qt::CaseInsensitive);
  completer_->setCompletionMode(QCompleter::UnfilteredPopupCompletion);
  completer_->setWrapAround(false);
  search_->setCompleter(completer_);
  completer_->popup()->setObjectName("marketSuggestions");
  wfgui::setCaptureTarget(completer_->popup(), "right-rail.market.suggestions");
  search_->installEventFilter(this);
  layout->addWidget(search_);

  empty_->setObjectName("marketRailEmpty");
  empty_->setText("Select an item or use search to view current listings.");
  empty_->setAlignment(Qt::AlignCenter);
  empty_->setWordWrap(true);
  layout->addWidget(empty_, 1);
  itemView_->hide();
  layout->addWidget(itemView_, 1);

  searchTimer_->setSingleShot(true);
  searchTimer_->setInterval(180);
  connect(search_, &QLineEdit::textEdited, this, [this](const QString &text) {
    latestQuery_ = text.trimmed();
    acceptingSuggestions_ = !latestQuery_.isEmpty();
    search_->setProperty(TypedTextProperty, text);
    suggestions_->clear();
    if (latestQuery_.isEmpty()) {
      searchTimer_->stop();
      completer_->popup()->hide();
      return;
    }
    searchTimer_->start();
  });
  connect(searchTimer_, &QTimer::timeout, this,
          &MarketRailWidget::requestSuggestions);
  connect(completer_, qOverload<const QModelIndex &>(&QCompleter::activated),
          this, &MarketRailWidget::activateSuggestion);
  connect(controller_, &AppController::marketSearchReady, this,
          &MarketRailWidget::updateSuggestions);
  connect(controller_, &AppController::marketSearchFailed, this,
          [this](const QString &query, const QString &error) {
            if (acceptingSuggestions_ &&
                query.compare(latestQuery_, Qt::CaseInsensitive) == 0 &&
                !itemView_->isVisible()) {
              empty_->setText(error);
            }
          });
  connect(itemView_, &MarketItemView::signInRequested, this,
          &MarketRailWidget::signInRequested);
}

void MarketRailWidget::showItem(const QString &item, const QString &side) {
  if (item.isEmpty()) {
    return;
  }
  const QString label =
      controller_->marketItem(item).value("name").toString(item);
  showResolvedItem(item, label, side);
}

bool MarketRailWidget::eventFilter(QObject *watched, QEvent *event) {
  if (watched != search_ || event->type() != QEvent::KeyPress) {
    return QWidget::eventFilter(watched, event);
  }

  auto *keyEvent = static_cast<QKeyEvent *>(event);
  QAbstractItemView *popup = completer_->popup();
  const int rowCount = popup->model()->rowCount();
  if (rowCount == 0) {
    return QWidget::eventFilter(watched, event);
  }

  if (keyEvent->key() == Qt::Key_Down || keyEvent->key() == Qt::Key_Up) {
    if (!popup->isVisible()) {
      completer_->complete();
      popup->setCurrentIndex({});
    }
    const int current = popup->currentIndex().row();
    const int row = keyEvent->key() == Qt::Key_Down
                        ? qMin(current + 1, rowCount - 1)
                        : (current < 0 ? rowCount - 1 : qMax(current - 1, 0));
    const QModelIndex index = popup->model()->index(row, 0);
    popup->setCurrentIndex(index);
    popup->scrollTo(index);
    return true;
  }

  if ((keyEvent->key() == Qt::Key_Return || keyEvent->key() == Qt::Key_Enter) &&
      popup->isVisible() && popup->currentIndex().isValid()) {
    activateSuggestion(popup->currentIndex());
    return true;
  }

  if (keyEvent->key() == Qt::Key_Escape && popup->isVisible()) {
    popup->hide();
    return true;
  }
  return QWidget::eventFilter(watched, event);
}

void MarketRailWidget::activateSuggestion(const QModelIndex &index) {
  const QString item = index.data(ItemKeyRole).toString();
  if (!item.isEmpty()) {
    showResolvedItem(item, index.data(Qt::DisplayRole).toString(), "sell");
  }
}

void MarketRailWidget::showResolvedItem(const QString &item,
                                        const QString &label,
                                        const QString &side) {
  searchTimer_->stop();
  completer_->popup()->hide();
  const QSignalBlocker blocker(search_);
  search_->setText(label);
  search_->setProperty(TypedTextProperty, label);
  latestQuery_ = label.trimmed();
  acceptingSuggestions_ = false;
  suggestions_->clear();
  empty_->hide();
  itemView_->show();
  itemView_->showItem(item, side);
}

void MarketRailWidget::requestSuggestions() {
  if (acceptingSuggestions_ && !latestQuery_.isEmpty()) {
    controller_->searchMarketItems(latestQuery_);
  }
}

void MarketRailWidget::updateSuggestions(const QString &query,
                                         const QJsonArray &matches) {
  if (!acceptingSuggestions_ ||
      query.compare(latestQuery_, Qt::CaseInsensitive) != 0) {
    return;
  }
  suggestions_->clear();
  for (const QJsonValue &value : matches) {
    const QJsonObject match = value.toObject();
    const QString name = match.value("name").toString();
    const QString key = match.value("slug").toString(name);
    if (name.isEmpty() || key.isEmpty()) {
      continue;
    }
    auto *item = new QStandardItem(name);
    item->setData(key, ItemKeyRole);
    suggestions_->appendRow(item);
  }
  if (suggestions_->rowCount() == 0) {
    if (!itemView_->isVisible()) {
      empty_->setText("No matching Market items.");
    }
    return;
  }
  if (search_->hasFocus()) {
    completer_->complete();
    completer_->popup()->setCurrentIndex({});
  }
}
