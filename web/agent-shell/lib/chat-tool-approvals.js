function normalizeToolName(name) {
  return String(name ?? '')
    .trim()
    .toLowerCase();
}

function buildToolAliases(name) {
  const normalized = normalizeToolName(name);
  if (!normalized) {
    return new Set();
  }

  const aliases = new Set([normalized]);
  const parts = normalized.split('.').filter(Boolean);

  for (let index = 1; index < parts.length; index += 1) {
    aliases.add(parts.slice(index).join('.'));
  }

  if (parts.length > 0) {
    aliases.add(parts.at(-1));
  }

  return aliases;
}

function toolNamesMatch(left, right) {
  const leftAliases = buildToolAliases(left);
  const rightAliases = buildToolAliases(right);

  for (const alias of leftAliases) {
    if (rightAliases.has(alias)) {
      return true;
    }
  }

  return false;
}

export function assignApprovalsToTools({ toolNodes, approvals }) {
  const pendingApprovals = [...approvals].sort(
    (left, right) => new Date(left.requested_at).getTime() - new Date(right.requested_at).getTime(),
  );
  const assignments = new Map();
  const consumedApprovalIds = new Set();

  for (const toolNode of toolNodes) {
    const approval = pendingApprovals.find(
      (candidate) =>
        !consumedApprovalIds.has(candidate.id) &&
        toolNamesMatch(toolNode.toolName, candidate.tool),
    );

    if (!approval) {
      continue;
    }

    assignments.set(toolNode.key, approval);
    consumedApprovalIds.add(approval.id);
  }

  return {
    assignments,
    unmatchedApprovals: pendingApprovals.filter((approval) => !consumedApprovalIds.has(approval.id)),
  };
}

export function getToolNameAliases(name) {
  return [...buildToolAliases(name)];
}
