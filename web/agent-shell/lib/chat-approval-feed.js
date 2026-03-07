function byDescendingIsoDate(left, right) {
  return new Date(right).getTime() - new Date(left).getTime();
}

export function buildApprovalFeed({ pending, resolved, resolvedLimit = 3 }) {
  const pendingIds = new Set(pending.map((approval) => approval.id));

  const pendingItems = [...pending]
    .sort((left, right) => byDescendingIsoDate(left.requested_at, right.requested_at))
    .map((approval) => ({ kind: 'pending', approval }));

  const resolvedItems = resolved
    .filter((approval) => !pendingIds.has(approval.id))
    .sort((left, right) => byDescendingIsoDate(left.resolvedAt, right.resolvedAt))
    .slice(0, resolvedLimit)
    .map((approval) => ({ kind: 'resolved', approval }));

  return [...pendingItems, ...resolvedItems];
}

export function approvalDecisionLabel(decision) {
  return decision === 'approve' ? 'Approved' : 'Denied';
}
