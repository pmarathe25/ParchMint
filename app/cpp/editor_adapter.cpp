#include "editor_adapter.h"

#include <QTextBlockFormat>
#include <QTextCharFormat>
#include <QTextDocument>

namespace {
constexpr int ParchMintPageBreakObject = QTextFormat::UserObject + 1;
constexpr int ParchMintObjectKind = QTextFormat::UserProperty + 1;
}

EditorAdapter::EditorAdapter(QObject* parent)
  : QObject(parent)
{
}

QObject* EditorAdapter::textDocument() const { return m_textDocument; }

void EditorAdapter::setTextDocument(QObject* object)
{
  auto* document = qobject_cast<QQuickTextDocument*>(object);
  if (object && !document) {
    emit adapterError(tr("The editor supplied an unsupported text document."));
    return;
  }
  if (m_textDocument == document)
    return;
  m_textDocument = document;
  m_document = document ? document->textDocument() : nullptr;
  m_cursorPosition = m_selectionStart = m_selectionEnd = 0;
  emit textDocumentChanged();
  emit cursorPositionChanged();
  emit selectionChanged();
  emit selectionFormatChanged();
}

void EditorAdapter::setDocumentForTesting(QTextDocument* document)
{
  m_textDocument = nullptr;
  m_document = document;
  m_cursorPosition = m_selectionStart = m_selectionEnd = 0;
  emit textDocumentChanged();
  emit cursorPositionChanged();
  emit selectionChanged();
  emit selectionFormatChanged();
}

int EditorAdapter::cursorPosition() const { return m_cursorPosition; }
int EditorAdapter::selectionStart() const { return m_selectionStart; }
int EditorAdapter::selectionEnd() const { return m_selectionEnd; }

void EditorAdapter::setCursorPosition(int position)
{
  if (!m_document)
    return;
  position = clampPosition(m_document, position);
  if (m_cursorPosition == position)
    return;
  m_cursorPosition = position;
  emit cursorPositionChanged();
  emit selectionFormatChanged();
}

void EditorAdapter::setSelectionStart(int position)
{
  if (!m_document)
    return;
  position = clampPosition(m_document, position);
  if (m_selectionStart == position)
    return;
  m_selectionStart = position;
  emit selectionChanged();
  emit selectionFormatChanged();
}

void EditorAdapter::setSelectionEnd(int position)
{
  if (!m_document)
    return;
  position = clampPosition(m_document, position);
  if (m_selectionEnd == position)
    return;
  m_selectionEnd = position;
  emit selectionChanged();
  emit selectionFormatChanged();
}

bool EditorAdapter::bold() const { return cursor().charFormat().fontWeight() >= QFont::Bold; }
bool EditorAdapter::italic() const { return cursor().charFormat().fontItalic(); }

QTextCursor EditorAdapter::cursor() const
{
  if (!m_document)
    return {};
  QTextCursor result(m_document);
  result.setPosition(clampPosition(result.document(), m_selectionStart));
  result.setPosition(clampPosition(result.document(), m_selectionEnd), QTextCursor::KeepAnchor);
  if (!result.hasSelection())
    result.setPosition(clampPosition(result.document(), m_cursorPosition));
  return result;
}

void EditorAdapter::toggleBold()
{
  QTextCharFormat format;
  format.setFontWeight(bold() ? QFont::Normal : QFont::Bold);
  mergeCharacterFormat(format);
}

void EditorAdapter::toggleItalic()
{
  QTextCharFormat format;
  format.setFontItalic(!italic());
  mergeCharacterFormat(format);
}

void EditorAdapter::setVerticalAlignment(int alignment)
{
  QTextCharFormat format;
  const auto value = alignment == 1 ? QTextCharFormat::AlignSuperScript
                                    : alignment == 2 ? QTextCharFormat::AlignSubScript
                                                     : QTextCharFormat::AlignNormal;
  format.setVerticalAlignment(value);
  mergeCharacterFormat(format);
}

void EditorAdapter::setParagraphAlignment(int alignment)
{
  auto textCursor = cursor();
  if (textCursor.isNull())
    return;
  QTextBlockFormat format;
  format.setAlignment(static_cast<Qt::Alignment>(alignment));
  textCursor.mergeBlockFormat(format);
  emit selectionFormatChanged();
}

void EditorAdapter::insertPageBreak()
{
  auto textCursor = cursor();
  if (textCursor.isNull()) {
    emit adapterError(tr("No editor document is connected."));
    return;
  }
  QTextCharFormat format;
  format.setObjectType(ParchMintPageBreakObject);
  format.setProperty(ParchMintObjectKind, QStringLiteral("parchmint:page-break"));
  format.setToolTip(tr("Page break (compile marker)"));
  textCursor.beginEditBlock();
  textCursor.insertText(QString(QChar::ObjectReplacementCharacter), format);
  textCursor.insertBlock();
  textCursor.endEditBlock();
}

void EditorAdapter::insertSceneBreak()
{
  auto textCursor = cursor();
  if (textCursor.isNull())
    return;
  textCursor.beginEditBlock();
  textCursor.insertBlock();
  QTextBlockFormat centered;
  centered.setAlignment(Qt::AlignHCenter);
  textCursor.setBlockFormat(centered);
  textCursor.insertText(QStringLiteral("* * *"));
  textCursor.insertBlock();
  textCursor.endEditBlock();
}

void EditorAdapter::beginGroupedEdit()
{
  if (m_groupedEdit)
    return;
  auto textCursor = cursor();
  if (!textCursor.isNull()) {
    textCursor.beginEditBlock();
    m_groupedEdit = true;
  }
}

void EditorAdapter::endGroupedEdit()
{
  if (!m_groupedEdit)
    return;
  auto textCursor = cursor();
  if (!textCursor.isNull())
    textCursor.endEditBlock();
  m_groupedEdit = false;
}

QString EditorAdapter::selectedPlainText() const { return cursor().selectedText(); }

void EditorAdapter::mergeCharacterFormat(const QTextCharFormat& format)
{
  auto textCursor = cursor();
  if (textCursor.isNull())
    return;
  textCursor.beginEditBlock();
  textCursor.mergeCharFormat(format);
  textCursor.endEditBlock();
  emit selectionFormatChanged();
}

int EditorAdapter::clampPosition(const QTextDocument* document, int position)
{
  return qBound(0, position, qMax(0, document->characterCount() - 1));
}
