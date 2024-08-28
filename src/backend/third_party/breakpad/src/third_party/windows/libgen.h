#define strdup _strdup

#ifdef __cplusplus
extern "C" {
#endif

char *basename(char *path);
char *dirname(char *path);

#ifdef __cplusplus
}
#endif
