/** Build-time capability: desktop always retains all catalog actions. */
export const isReadOnly = () => import.meta.env.VITE_TARGET === 'web';

/** Keep in sync with katalog-server's is_allowed (verified against Rust source). */
export const WEB_READ_COMMANDS = [
  'list_file_summaries', 'list_all_file_tags', 'list_files_by_ids', 'list_tag_counts',
  'list_print_log_entries', 'list_folders', 'list_collections', 'list_collection_files',
  'list_trash', 'list_file_images', 'get_model_geometry', 'get_app_version',
  'is_preview_build', 'has_step_preview',
] as const;
