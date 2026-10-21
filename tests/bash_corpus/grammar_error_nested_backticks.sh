case $KIND in
  one)
    NAME="`echo $ITEM | cut -d : -f 3`"
    printf "%s\n" "$NAME" >&2
    LIST="items: ` run $ITEM | paste -s`"
    ;;
esac
