#include "semantic_object_renderer.h"

#include <QPainter>
#include <QTextDocument>
#include <QTextFormat>

namespace {
constexpr int ParchMintObjectKind = QTextFormat::UserProperty + 1;
}

SemanticObjectRenderer::SemanticObjectRenderer(QObject* parent)
  : QObject(parent)
{
}

QSizeF SemanticObjectRenderer::intrinsicSize(QTextDocument*, int, const QTextFormat& format)
{
  const auto opaque = format.property(ParchMintObjectKind).toString()
                      == QStringLiteral("parchmint:opaque");
  return opaque ? QSizeF(190, 28) : QSizeF(150, 24);
}

void SemanticObjectRenderer::drawObject(QPainter* painter,
                                        const QRectF& rect,
                                        QTextDocument*,
                                        int,
                                        const QTextFormat& format)
{
  painter->save();
  const auto opaque = format.property(ParchMintObjectKind).toString()
                      == QStringLiteral("parchmint:opaque");
  const QColor foreground = opaque ? QColor(122, 71, 25) : QColor(74, 85, 104);
  const QColor background = opaque ? QColor(255, 244, 225) : QColor(237, 242, 247);
  painter->setPen(QPen(foreground, 1));
  painter->setBrush(background);
  painter->drawRoundedRect(rect.adjusted(0.5, 0.5, -0.5, -0.5), 4, 4);
  painter->drawText(rect.adjusted(8, 0, -8, 0),
                    Qt::AlignVCenter | Qt::AlignLeft,
                    opaque ? tr("Unsupported Markdown — protected") : tr("Page break"));
  painter->restore();
}
