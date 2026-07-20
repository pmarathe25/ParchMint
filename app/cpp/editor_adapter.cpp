#include "editor_adapter.h"
#include "semantic_object_renderer.h"

#include <QAbstractTextDocumentLayout>
#include <QFont>
#include <QTextBlock>
#include <QTextBlockFormat>
#include <QTextCharFormat>
#include <QTextDocument>
#include <QTextDocumentFragment>
#include <QTextImageFormat>
#include <QTextList>
#include <QTextListFormat>
#include <QUrl>
#include <QVariantMap>

namespace {
constexpr int ParchMintPageBreakObject = QTextFormat::UserObject + 1;
constexpr int ParchMintOpaqueObject = QTextFormat::UserObject + 2;
constexpr int ParchMintObjectKind = QTextFormat::UserProperty + 1;
constexpr int ParchMintStableStyle = QTextFormat::UserProperty + 10;
constexpr int ParchMintOpaqueSource = QTextFormat::UserProperty + 11;
constexpr int ParchMintImageAlt = QTextFormat::UserProperty + 12;
constexpr int ParchMintProtected = QTextFormat::UserProperty + 13;
constexpr int ParchMintDirectFormatting = QTextFormat::UserProperty + 14;

bool isProtected(const QTextCharFormat& format)
{
  return format.property(ParchMintProtected).toBool()
    || format.objectType() == ParchMintPageBreakObject
    || format.objectType() == ParchMintOpaqueObject;
}

void insertRuns(QTextCursor& cursor, const QVariantMap& block)
{
  const auto runs = block.value(QStringLiteral("runs")).toList();
  if (runs.isEmpty()) {
    cursor.insertText(block.value(QStringLiteral("text")).toString());
    return;
  }
  for (const auto& value : runs) {
    const auto run = value.toMap();
    QTextCharFormat format;
    format.setFontWeight(run.value(QStringLiteral("bold")).toBool() ? QFont::Bold : QFont::Normal);
    format.setFontItalic(run.value(QStringLiteral("italic")).toBool());
    format.setFontStrikeOut(run.value(QStringLiteral("strike")).toBool());
    if (run.value(QStringLiteral("superscript")).toBool())
      format.setVerticalAlignment(QTextCharFormat::AlignSuperScript);
    else if (run.value(QStringLiteral("subscript")).toBool())
      format.setVerticalAlignment(QTextCharFormat::AlignSubScript);
    const auto style = run.value(QStringLiteral("styleId")).toString();
    if (!style.isEmpty())
      format.setProperty(ParchMintStableStyle, style);
    const auto destination = run.value(QStringLiteral("link")).toString();
    if (!destination.isEmpty()) {
      format.setAnchor(true);
      format.setAnchorHref(destination);
    }
    cursor.insertText(run.value(QStringLiteral("text")).toString(), format);
  }
}

void applyCharacterAppearance(QTextCharFormat& format, const QVariantMap& properties)
{
  if (properties.contains(QStringLiteral("font-family")))
    format.setFontFamilies({ properties.value(QStringLiteral("font-family")).toString() });
  if (properties.contains(QStringLiteral("font-size")))
    format.setFontPointSize(properties.value(QStringLiteral("font-size")).toDouble());
  if (properties.contains(QStringLiteral("font-weight")))
    format.setFontWeight(properties.value(QStringLiteral("font-weight")).toInt());
  if (properties.contains(QStringLiteral("font-style")))
    format.setFontItalic(properties.value(QStringLiteral("font-style")).toString()
                         == QStringLiteral("italic"));
  if (properties.contains(QStringLiteral("foreground")))
    format.setForeground(QColor(properties.value(QStringLiteral("foreground")).toString()));
  if (properties.contains(QStringLiteral("background")))
    format.setBackground(QColor(properties.value(QStringLiteral("background")).toString()));
}
}

EditorAdapter::EditorAdapter(QObject* parent)
  : QObject(parent)
  , m_objectRenderer(new SemanticObjectRenderer(this))
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
  if (m_document)
    disconnect(m_document, nullptr, this, nullptr);
  m_textDocument = document;
  m_document = document ? document->textDocument() : nullptr;
  m_cursorPosition = m_selectionStart = m_selectionEnd = 0;
  m_revision = 0;
  connectDocumentSignals();
  emit textDocumentChanged();
  emit cursorPositionChanged();
  emit selectionChanged();
  emit selectionFormatChanged();
  emit revisionChanged();
}

void EditorAdapter::setDocumentForTesting(QTextDocument* document)
{
  if (m_document)
    disconnect(m_document, nullptr, this, nullptr);
  m_textDocument = nullptr;
  m_document = document;
  m_cursorPosition = m_selectionStart = m_selectionEnd = 0;
  m_revision = 0;
  connectDocumentSignals();
  emit textDocumentChanged();
  emit cursorPositionChanged();
  emit selectionChanged();
  emit selectionFormatChanged();
  emit revisionChanged();
}

void EditorAdapter::connectDocumentSignals()
{
  if (!m_document)
    return;
  // `contentsChange` is produced through the document layout notification
  // path; ensure isolated harness documents have a layout just like QML hosts.
  (void)m_document->documentLayout();
  m_document->documentLayout()->registerHandler(ParchMintPageBreakObject, m_objectRenderer);
  m_document->documentLayout()->registerHandler(ParchMintOpaqueObject, m_objectRenderer);
  connect(m_document, &QTextDocument::contentsChange, this, &EditorAdapter::onContentsChange);
  connect(m_document,
          &QTextDocument::undoAvailable,
          this,
          &EditorAdapter::undoAvailabilityChanged);
  connect(m_document,
          &QTextDocument::redoAvailable,
          this,
          &EditorAdapter::redoAvailabilityChanged);
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

bool EditorAdapter::bold() const { return boldState() == 1; }
bool EditorAdapter::italic() const { return italicState() == 1; }

int EditorAdapter::boldState() const
{
  if (!m_document)
    return 0;
  const auto selected = cursor();
  const int start = selected.hasSelection() ? selected.selectionStart() : selected.position();
  const int end = selected.hasSelection() ? selected.selectionEnd() : start + 1;
  int state = -2;
  for (auto block = m_document->findBlock(start); block.isValid() && block.position() < end;
       block = block.next()) {
    for (auto it = block.begin(); !it.atEnd(); ++it) {
      const auto fragment = it.fragment();
      if (!fragment.isValid() || fragment.position() + fragment.length() <= start
          || fragment.position() >= end)
        continue;
      const int value = fragment.charFormat().fontWeight() >= QFont::Bold ? 1 : 0;
      if (state == -2)
        state = value;
      else if (state != value)
        return -1;
    }
  }
  return state == -2 ? (selected.charFormat().fontWeight() >= QFont::Bold ? 1 : 0) : state;
}

int EditorAdapter::italicState() const
{
  if (!m_document)
    return 0;
  const auto selected = cursor();
  const int start = selected.hasSelection() ? selected.selectionStart() : selected.position();
  const int end = selected.hasSelection() ? selected.selectionEnd() : start + 1;
  int state = -2;
  for (auto block = m_document->findBlock(start); block.isValid() && block.position() < end;
       block = block.next()) {
    for (auto it = block.begin(); !it.atEnd(); ++it) {
      const auto fragment = it.fragment();
      if (!fragment.isValid() || fragment.position() + fragment.length() <= start
          || fragment.position() >= end)
        continue;
      const int value = fragment.charFormat().fontItalic() ? 1 : 0;
      if (state == -2)
        state = value;
      else if (state != value)
        return -1;
    }
  }
  return state == -2 ? (selected.charFormat().fontItalic() ? 1 : 0) : state;
}

int EditorAdapter::paragraphAlignment() const
{
  const auto textCursor = cursor();
  return textCursor.isNull() ? 0 : static_cast<int>(textCursor.blockFormat().alignment());
}

QString EditorAdapter::paragraphStyle() const
{
  const auto textCursor = cursor();
  return textCursor.isNull()
    ? QString()
    : textCursor.blockFormat().property(ParchMintStableStyle).toString();
}

bool EditorAdapter::canUndo() const { return m_focused && m_document && m_document->isUndoAvailable(); }
bool EditorAdapter::canRedo() const { return m_focused && m_document && m_document->isRedoAvailable(); }
bool EditorAdapter::focused() const { return m_focused; }

void EditorAdapter::setFocused(bool focused)
{
  if (m_focused == focused)
    return;
  m_focused = focused;
  emit focusedChanged();
  emit undoAvailabilityChanged();
  emit redoAvailabilityChanged();
  if (!focused)
    emit focusLostFlushRequested(m_revision);
}

qulonglong EditorAdapter::revision() const { return m_revision; }

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

void EditorAdapter::loadSemanticBlocks(const QVariantList& blocks)
{
  if (!m_document) {
    emit adapterError(tr("No editor document is connected."));
    return;
  }
  m_loading = true;
  QTextCursor textCursor(m_document);
  textCursor.beginEditBlock();
  textCursor.select(QTextCursor::Document);
  textCursor.removeSelectedText();
  bool first = true;
  for (const auto& value : blocks) {
    const auto block = value.toMap();
    const auto type = block.value(QStringLiteral("type"), QStringLiteral("paragraph")).toString();
    if (!first)
      textCursor.insertBlock();
    first = false;
    if (type == QStringLiteral("page_break")) {
      QTextCharFormat format;
      format.setObjectType(ParchMintPageBreakObject);
      format.setProperty(ParchMintObjectKind, QStringLiteral("parchmint:page-break"));
      format.setProperty(ParchMintProtected, true);
      format.setToolTip(tr("Page break (compile marker)"));
      textCursor.insertText(QString(QChar::ObjectReplacementCharacter), format);
      continue;
    }
    if (type == QStringLiteral("opaque")) {
      QTextCharFormat format;
      format.setObjectType(ParchMintOpaqueObject);
      format.setProperty(ParchMintObjectKind, QStringLiteral("parchmint:opaque"));
      format.setProperty(ParchMintProtected, true);
      format.setProperty(ParchMintOpaqueSource, block.value(QStringLiteral("source")));
      format.setToolTip(block.value(QStringLiteral("reason")).toString());
      textCursor.insertText(QString(QChar::ObjectReplacementCharacter), format);
      continue;
    }
    if (type == QStringLiteral("image")) {
      QTextImageFormat image;
      image.setName(block.value(QStringLiteral("assetId")).toString());
      image.setProperty(ParchMintImageAlt, block.value(QStringLiteral("alt")));
      textCursor.insertImage(image);
      continue;
    }
    QTextBlockFormat format;
    if (type == QStringLiteral("heading"))
      format.setHeadingLevel(qBound(1, block.value(QStringLiteral("level"), 1).toInt(), 6));
    format.setAlignment(static_cast<Qt::Alignment>(
      block.value(QStringLiteral("alignment"), static_cast<int>(Qt::AlignLeft)).toInt()));
    const auto style = block.value(QStringLiteral("styleId")).toString();
    if (!style.isEmpty())
      format.setProperty(ParchMintStableStyle, style);
    if (type == QStringLiteral("thematic_break")) {
      format.setAlignment(Qt::AlignHCenter);
      format.setProperty(ParchMintObjectKind, QStringLiteral("parchmint:thematic-break"));
    }
    textCursor.setBlockFormat(format);
    if (type == QStringLiteral("list")) {
      QTextListFormat listFormat;
      listFormat.setStyle(block.value(QStringLiteral("ordered")).toBool()
                            ? QTextListFormat::ListDecimal
                            : QTextListFormat::ListDisc);
      textCursor.createList(listFormat);
    }
    if (type == QStringLiteral("thematic_break")
        && block.value(QStringLiteral("text")).toString().isEmpty())
      textCursor.insertText(QStringLiteral("* * *"));
    else
      insertRuns(textCursor, block);
  }
  textCursor.endEditBlock();
  m_document->clearUndoRedoStacks();
  m_revision = 0;
  m_loading = false;
  emit revisionChanged();
  emit selectionFormatChanged();
}

QVariantList EditorAdapter::semanticBlocks() const
{
  QVariantList result;
  if (!m_document)
    return result;
  for (auto block = m_document->begin(); block.isValid(); block = block.next()) {
    QVariantMap output;
    output.insert(QStringLiteral("type"),
                  block.blockFormat().headingLevel() > 0 ? QStringLiteral("heading")
                                                         : QStringLiteral("paragraph"));
    if (block.blockFormat().headingLevel() > 0)
      output.insert(QStringLiteral("level"), block.blockFormat().headingLevel());
    output.insert(QStringLiteral("alignment"), static_cast<int>(block.blockFormat().alignment()));
    output.insert(QStringLiteral("styleId"),
                  block.blockFormat().property(ParchMintStableStyle).toString());
    QVariantList runs;
    for (auto it = block.begin(); !it.atEnd(); ++it) {
      const auto fragment = it.fragment();
      if (!fragment.isValid())
        continue;
      const auto format = fragment.charFormat();
      if (format.objectType() == ParchMintPageBreakObject) {
        output.clear();
        output.insert(QStringLiteral("type"), QStringLiteral("page_break"));
        runs.clear();
        break;
      }
      if (format.objectType() == ParchMintOpaqueObject) {
        output.clear();
        output.insert(QStringLiteral("type"), QStringLiteral("opaque"));
        output.insert(QStringLiteral("source"), format.property(ParchMintOpaqueSource));
        output.insert(QStringLiteral("reason"), format.toolTip());
        runs.clear();
        break;
      }
      if (format.isImageFormat()) {
        const auto image = format.toImageFormat();
        output.clear();
        output.insert(QStringLiteral("type"), QStringLiteral("image"));
        output.insert(QStringLiteral("assetId"), image.name());
        output.insert(QStringLiteral("alt"), image.property(ParchMintImageAlt));
        runs.clear();
        break;
      }
      QVariantMap run;
      run.insert(QStringLiteral("text"), fragment.text());
      run.insert(QStringLiteral("bold"), format.fontWeight() >= QFont::Bold);
      run.insert(QStringLiteral("italic"), format.fontItalic());
      run.insert(QStringLiteral("strike"), format.fontStrikeOut());
      run.insert(QStringLiteral("superscript"),
                 format.verticalAlignment() == QTextCharFormat::AlignSuperScript);
      run.insert(QStringLiteral("subscript"),
                 format.verticalAlignment() == QTextCharFormat::AlignSubScript);
      run.insert(QStringLiteral("styleId"), format.property(ParchMintStableStyle));
      run.insert(QStringLiteral("link"), format.anchorHref());
      runs.push_back(run);
    }
    if (!runs.isEmpty())
      output.insert(QStringLiteral("runs"), runs);
    if (block.blockFormat().property(ParchMintObjectKind).toString()
        == QStringLiteral("parchmint:thematic-break"))
      output.insert(QStringLiteral("type"), QStringLiteral("thematic_break"));
    if (block.textList()) {
      output.insert(QStringLiteral("type"), QStringLiteral("list"));
      output.insert(QStringLiteral("ordered"),
                    block.textList()->format().style() == QTextListFormat::ListDecimal);
    }
    result.push_back(output);
  }
  return result;
}

void EditorAdapter::toggleBold()
{
  QTextCharFormat format;
  format.setFontWeight(boldState() == 1 ? QFont::Normal : QFont::Bold);
  format.setProperty(ParchMintDirectFormatting, true);
  mergeCharacterFormat(format);
}

void EditorAdapter::toggleItalic()
{
  QTextCharFormat format;
  format.setFontItalic(italicState() != 1);
  format.setProperty(ParchMintDirectFormatting, true);
  mergeCharacterFormat(format);
}

void EditorAdapter::setVerticalAlignment(int alignment)
{
  QTextCharFormat format;
  const auto value = alignment == 1 ? QTextCharFormat::AlignSuperScript
                                    : alignment == 2 ? QTextCharFormat::AlignSubScript
                                                     : QTextCharFormat::AlignNormal;
  format.setVerticalAlignment(value);
  format.setProperty(ParchMintDirectFormatting, true);
  mergeCharacterFormat(format);
}

void EditorAdapter::setParagraphAlignment(int alignment)
{
  auto textCursor = cursor();
  if (textCursor.isNull())
    return;
  QTextBlockFormat format;
  format.setAlignment(static_cast<Qt::Alignment>(alignment));
  format.setProperty(ParchMintDirectFormatting, true);
  textCursor.beginEditBlock();
  textCursor.mergeBlockFormat(format);
  textCursor.endEditBlock();
  emit selectionFormatChanged();
}

void EditorAdapter::setHeadingLevel(int level)
{
  auto textCursor = cursor();
  if (textCursor.isNull())
    return;
  QTextBlockFormat format;
  format.setHeadingLevel(qBound(0, level, 6));
  textCursor.beginEditBlock();
  textCursor.mergeBlockFormat(format);
  textCursor.endEditBlock();
  emit selectionFormatChanged();
}

void EditorAdapter::defineStyle(const QString& styleId,
                                const QVariantMap& properties,
                                bool paragraph,
                                const QString& nextStyleId)
{
  if (styleId.trimmed().isEmpty()) {
    emit adapterError(tr("A style must have a stable identifier."));
    return;
  }
  m_styleDefinitions.insert(styleId, properties);
  m_paragraphStyles.insert(styleId, paragraph);
  m_nextStyles.insert(styleId, nextStyleId);
  if (paragraphStyle() == styleId)
    setParagraphStyle(styleId);
}

void EditorAdapter::setParagraphStyle(const QString& styleId)
{
  auto textCursor = cursor();
  if (textCursor.isNull())
    return;
  if (m_paragraphStyles.contains(styleId) && !m_paragraphStyles.value(styleId)) {
    emit adapterError(tr("A character style cannot be applied to a paragraph."));
    return;
  }
  const auto properties = m_styleDefinitions.value(styleId);
  QTextBlockFormat format;
  format.setProperty(ParchMintStableStyle, styleId);
  const auto alignment = properties.value(QStringLiteral("alignment")).toString();
  if (alignment == QStringLiteral("center"))
    format.setAlignment(Qt::AlignHCenter);
  else if (alignment == QStringLiteral("right"))
    format.setAlignment(Qt::AlignRight);
  else if (alignment == QStringLiteral("justify"))
    format.setAlignment(Qt::AlignJustify);
  else if (alignment == QStringLiteral("left"))
    format.setAlignment(Qt::AlignLeft);
  QTextCharFormat character;
  applyCharacterAppearance(character, properties);
  textCursor.beginEditBlock();
  textCursor.mergeBlockFormat(format);
  if (!properties.isEmpty()) {
    auto wholeBlocks = textCursor;
    wholeBlocks.movePosition(QTextCursor::StartOfBlock);
    wholeBlocks.movePosition(QTextCursor::EndOfBlock, QTextCursor::KeepAnchor);
    wholeBlocks.mergeCharFormat(character);
  }
  textCursor.endEditBlock();
  emit selectionFormatChanged();
}

void EditorAdapter::setCharacterStyle(const QString& styleId)
{
  if (m_paragraphStyles.value(styleId, false)) {
    emit adapterError(tr("A paragraph style cannot be applied to characters."));
    return;
  }
  QTextCharFormat format;
  format.setProperty(ParchMintStableStyle, styleId);
  applyCharacterAppearance(format, m_styleDefinitions.value(styleId));
  mergeCharacterFormat(format);
}

void EditorAdapter::clearDirectFormatting()
{
  auto textCursor = cursor();
  if (textCursor.isNull())
    return;
  QTextCharFormat character;
  character.setFontWeight(QFont::Normal);
  character.setFontItalic(false);
  character.setFontStrikeOut(false);
  character.setFontUnderline(false);
  character.setVerticalAlignment(QTextCharFormat::AlignNormal);
  character.clearForeground();
  character.clearBackground();
  character.clearProperty(ParchMintDirectFormatting);
  QTextBlockFormat paragraph;
  paragraph.clearProperty(ParchMintDirectFormatting);
  textCursor.beginEditBlock();
  textCursor.mergeCharFormat(character);
  textCursor.mergeBlockFormat(paragraph);
  textCursor.endEditBlock();
  emit selectionFormatChanged();
}

void EditorAdapter::toggleList(bool ordered)
{
  auto textCursor = cursor();
  if (textCursor.isNull())
    return;
  textCursor.beginEditBlock();
  if (textCursor.currentList()) {
    QTextBlockFormat block = textCursor.blockFormat();
    block.setObjectIndex(-1);
    textCursor.setBlockFormat(block);
  } else {
    QTextListFormat format;
    format.setStyle(ordered ? QTextListFormat::ListDecimal : QTextListFormat::ListDisc);
    textCursor.createList(format);
  }
  textCursor.endEditBlock();
}

void EditorAdapter::setLink(const QString& destination)
{
  const QUrl url(destination);
  if (!destination.isEmpty()
      && (!url.isValid()
          || (!url.scheme().isEmpty() && url.scheme() != QStringLiteral("https")
              && url.scheme() != QStringLiteral("http")
              && url.scheme() != QStringLiteral("mailto")
              && url.scheme() != QStringLiteral("asset")))) {
    emit adapterError(tr("The link uses an unsupported or unsafe destination."));
    return;
  }
  QTextCharFormat format;
  format.setAnchor(!destination.isEmpty());
  format.setAnchorHref(destination);
  mergeCharacterFormat(format);
}

void EditorAdapter::insertImage(const QString& assetId, const QString& altText)
{
  auto textCursor = cursor();
  if (textCursor.isNull())
    return;
  QTextImageFormat image;
  image.setName(assetId.startsWith(QStringLiteral("asset:")) ? assetId
                                                              : QStringLiteral("asset:") + assetId);
  image.setProperty(ParchMintImageAlt, altText);
  textCursor.insertImage(image);
}

void EditorAdapter::insertOpaqueBlock(const QString& source, const QString& reason)
{
  auto textCursor = cursor();
  if (textCursor.isNull())
    return;
  QTextCharFormat format;
  format.setObjectType(ParchMintOpaqueObject);
  format.setProperty(ParchMintObjectKind, QStringLiteral("parchmint:opaque"));
  format.setProperty(ParchMintOpaqueSource, source);
  format.setProperty(ParchMintProtected, true);
  format.setToolTip(reason);
  textCursor.beginEditBlock();
  textCursor.insertText(QString(QChar::ObjectReplacementCharacter), format);
  textCursor.insertBlock();
  textCursor.endEditBlock();
}

void EditorAdapter::insertParagraphBreak()
{
  auto textCursor = cursor();
  if (textCursor.isNull())
    return;
  const auto currentStyle = textCursor.blockFormat().property(ParchMintStableStyle).toString();
  const auto nextStyle = m_nextStyles.value(currentStyle, currentStyle);
  textCursor.beginEditBlock();
  textCursor.insertBlock();
  if (!nextStyle.isEmpty()) {
    QTextBlockFormat format;
    format.setProperty(ParchMintStableStyle, nextStyle);
    textCursor.mergeBlockFormat(format);
  }
  textCursor.endEditBlock();
  m_cursorPosition = textCursor.position();
  m_selectionStart = m_selectionEnd = m_cursorPosition;
  emit cursorPositionChanged();
  emit selectionChanged();
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
  format.setProperty(ParchMintProtected, true);
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
  centered.setProperty(ParchMintObjectKind, QStringLiteral("parchmint:thematic-break"));
  textCursor.setBlockFormat(centered);
  textCursor.insertText(QStringLiteral("* * *"));
  textCursor.insertBlock();
  textCursor.endEditBlock();
}

bool EditorAdapter::selectionContainsProtectedObject(const QTextCursor& selected) const
{
  if (!selected.hasSelection() || !m_document)
    return false;
  for (int position = selected.selectionStart(); position < selected.selectionEnd(); ++position) {
    QTextCursor probe(m_document);
    probe.setPosition(position);
    probe.movePosition(QTextCursor::NextCharacter, QTextCursor::KeepAnchor);
    if (isProtected(probe.charFormat()))
      return true;
  }
  return false;
}

void EditorAdapter::deletePreviousSemanticUnit()
{
  auto textCursor = cursor();
  if (textCursor.isNull())
    return;
  textCursor.beginEditBlock();
  if (textCursor.hasSelection()) {
    // Calling this invokable is the explicit user action which authorizes
    // deleting protected objects within the selection.
    textCursor.removeSelectedText();
  } else if (textCursor.movePosition(QTextCursor::PreviousCharacter, QTextCursor::KeepAnchor)) {
    textCursor.removeSelectedText();
  }
  textCursor.endEditBlock();
}

void EditorAdapter::pastePlainText(const QString& text)
{
  auto textCursor = cursor();
  if (!textCursor.isNull())
    textCursor.insertFragment(QTextDocumentFragment::fromPlainText(text));
}

void EditorAdapter::pasteRichHtml(const QString& html)
{
  auto textCursor = cursor();
  if (textCursor.isNull())
    return;
  // Qt's fragment importer drops scripts and active elements. Semantic save
  // later retains only formats represented by `semanticBlocks()`.
  textCursor.insertFragment(QTextDocumentFragment::fromHtml(html));
}

void EditorAdapter::undo()
{
  if (canUndo())
    m_document->undo();
}

void EditorAdapter::redo()
{
  if (canRedo())
    m_document->redo();
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
  if (selectionContainsProtectedObject(textCursor)) {
    emit adapterError(tr("Protected source and page-break objects cannot be formatted."));
    return;
  }
  textCursor.beginEditBlock();
  textCursor.mergeCharFormat(format);
  textCursor.endEditBlock();
  emit selectionFormatChanged();
}

int EditorAdapter::uniformCharacterProperty(int property, const QVariant& enabledValue) const
{
  const auto textCursor = cursor();
  if (textCursor.isNull())
    return 0;
  return textCursor.charFormat().property(property) == enabledValue ? 1 : 0;
}

void EditorAdapter::onContentsChange(int position, int removed, int added)
{
  if (m_loading || !m_document)
    return;
  ++m_revision;
  const auto first = m_document->findBlock(qMax(0, position)).blockNumber();
  const auto last = m_document->findBlock(qMax(0, position + added)).blockNumber() + 1;
  emit revisionChanged();
  emit incrementalDirty(m_revision, position, removed, added, qMax(0, first), qMax(first + 1, last));
}

int EditorAdapter::clampPosition(const QTextDocument* document, int position)
{
  return qBound(0, position, qMax(0, document->characterCount() - 1));
}
