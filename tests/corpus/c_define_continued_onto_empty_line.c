/* A definition whose last continued line is empty keeps that line, and
   the blank line after it stays a blank line. */
#define ALTERNATIVE(A) \
	if (A) { \
		return 1; \
	} \


#ifdef WITH_TLS
int tls;
#endif
