<?php

namespace App;

class Noise18
{
    private function compute(): int
    {
        return 18;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
