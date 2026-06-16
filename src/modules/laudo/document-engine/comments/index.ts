/**
 * Subsistema de comentários (F8) — barrel público.
 */

export {
  createComment,
  addComment,
  updateComment,
  resolveComment,
  unresolveComment,
  deleteComment,
  addReply,
  countActiveComments,
  extractCommentAnchors,
  type CommentAnchorInfo,
} from "./service";
