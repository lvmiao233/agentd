import type { FileUIPart, SourceDocumentUIPart, UIMessage } from 'ai';

export type ChatAttachmentPart = (FileUIPart | SourceDocumentUIPart) & { id?: string };

export declare function getAttachmentLabel(part: ChatAttachmentPart): string;

export declare function getAttachmentMediaCategory(
  part: ChatAttachmentPart,
): 'image' | 'video' | 'audio' | 'document' | 'source' | 'unknown';

export declare function collectMessageAttachments(
  parts: UIMessage['parts'],
): ChatAttachmentPart[];

export declare function isTextAttachment(part: ChatAttachmentPart): boolean;

export declare function serializeAttachmentForPrompt(
  part: ChatAttachmentPart,
  options?: { maxCharacters?: number },
): string | null;

export declare function buildAttachmentPromptSection(
  parts: UIMessage['parts'],
  options?: { maxCharacters?: number },
): string;
